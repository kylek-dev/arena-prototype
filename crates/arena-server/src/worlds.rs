//! Hosting hub worlds ([ADR 0057](../../../docs/decisions/0057-hub-on-the-world-server.md)):
//! the registry of live worlds, each world's task, and the instance
//! directory the front door asks once a second.
//!
//! A world starts when the first `Role::Hub` ticket for its instance id
//! arrives (as a match does) and ends after it has been empty for
//! [`WorldLimits::idle`](crate::world::WorldLimits); a later ticket for the
//! same id starts a fresh one. `POST /worlds/report` is the directory: the
//! door sends the blocked pairs among the players it placed (signed with
//! the ticket key, with a timestamp that must be recent and newer than the
//! last one) and gets back every world's instance, players and capacity.

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arena_protocol::{Handle, InputFrame, Tags, WireAppearance};
use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;
use tokio::time::MissedTickBehavior;

use crate::logging::MatchId;
use crate::metrics::Metrics;
use crate::session::{ConnId, Out};
use crate::ticket::Claims;
use crate::world::{Allowed, BlockSet, World, WorldReport, block_set};
use crate::{App, Conns, Outbound, deliver, insert_conn, remove_conn, unix_now};
use arena_protocol::ErrorCode;

/// Header carrying [`crate::ticket::TicketKey::sign`]'s tag of a directory
/// request body.
pub const DIRECTORY_SIGNATURE: &str = "x-arena-signature";
/// Domain label of directory request signatures.
pub const DIRECTORY_LABEL: &str = "world-directory";
/// How far a directory request's clock may be from the server's.
pub const DIRECTORY_SKEW_MS: u64 = 60_000;
/// Largest directory request body.
pub const MAX_DIRECTORY_BODY: usize = 256 * 1024;

/// `POST /worlds/report`'s body, from the front door.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryRequest {
    /// The door's clock in Unix milliseconds: within
    /// [`DIRECTORY_SKEW_MS`] of the server's and above the last accepted
    /// request's, so a recorded request cannot be replayed.
    pub now_ms: u64,
    /// Every blocked pair among players the door has placed in worlds, by
    /// the ticket player ids it issued.
    pub blocks: Vec<(u64, u64)>,
    /// Every player id the door has placed (in any world). When given,
    /// nobody else may join or stay in a world: a ticket outlives the
    /// door's placement, so this is how leaving the hub, taking a match or
    /// being forgotten takes effect on the world server.
    #[serde(default)]
    pub players: Option<Vec<u64>>,
}

/// `POST /worlds/report`'s answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DirectoryReply {
    /// The server is stopping: place nobody new here.
    #[serde(default)]
    pub draining: bool,
    pub worlds: Vec<WorldReport>,
}

pub(crate) enum WorldCommand {
    Attach {
        player: u64,
        conn: ConnId,
        handle: Option<Handle>,
        tags: Tags,
        appearance: Option<Box<WireAppearance>>,
        out: mpsc::Sender<Outbound>,
    },
    Input {
        conn: ConnId,
        frame: InputFrame,
    },
    Detach {
        conn: ConnId,
    },
    Directory {
        blocks: BlockSet,
        allowed: Allowed,
    },
}

/// Commands buffered per world; readers wait when it is full.
const COMMAND_QUEUE: usize = 1024;

pub(crate) struct WorldEntry {
    sender: mpsc::Sender<WorldCommand>,
    report: Arc<Mutex<WorldReport>>,
}

#[derive(Default)]
pub(crate) struct Worlds {
    live: HashMap<u64, WorldEntry>,
}

impl App {
    /// The world a verified hub ticket names, starting it if this is the
    /// first ticket to arrive. Must be called inside the Tokio runtime.
    pub(crate) fn world_for(
        self: &Arc<Self>,
        claims: &Claims,
    ) -> Result<mpsc::Sender<WorldCommand>, ErrorCode> {
        if !claims.is_valid() || !claims.is_hub() {
            return Err(ErrorCode::BadTicket);
        }
        let mut worlds = self.worlds.lock().expect("world registry");
        if let Some(entry) = worlds.live.get(&claims.match_id) {
            return Ok(entry.sender.clone());
        }
        if self.is_draining() {
            return Err(ErrorCode::ServerShutdown);
        }
        let id = claims.match_id;
        let (blocks, allowed) = self
            .directory_blocks
            .lock()
            .expect("directory blocks")
            .clone();
        let world = World::new(
            id,
            blocks,
            allowed,
            self.config.world_limits,
            self.metrics.clone(),
        );
        let report = Arc::new(Mutex::new(world.report()));
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE);
        worlds.live.insert(
            id,
            WorldEntry {
                sender: sender.clone(),
                report: report.clone(),
            },
        );
        drop(worlds);
        Metrics::add(&self.metrics.worlds_active, 1);
        tracing::info!(match_id = %MatchId(id), "hub world started by its first connection");
        tokio::spawn(run_world(self.clone(), id, world, receiver, report));
        Ok(sender)
    }

    /// Every live world's report, for the directory.
    pub fn world_reports(&self) -> Vec<WorldReport> {
        let worlds = self.worlds.lock().expect("world registry");
        let mut reports: Vec<WorldReport> = worlds
            .live
            .values()
            .map(|entry| entry.report.lock().expect("world report").clone())
            .collect();
        reports.sort_by(|a, b| a.instance.cmp(&b.instance));
        reports
    }

    fn world_senders(&self) -> Vec<mpsc::Sender<WorldCommand>> {
        let worlds = self.worlds.lock().expect("world registry");
        worlds
            .live
            .values()
            .map(|entry| entry.sender.clone())
            .collect()
    }
}

/// Delivers a world's output. `Member` and `MemberLeft` are never repeated
/// (unlike snapshots), so a connection too slow to take one is closed: its
/// page reconnects and is sent the whole member list again.
fn deliver_world(app: &App, id: u64, conns: &mut Conns, out: Vec<Out>) {
    // In order: a run of ordinary output goes out together, and each
    // member frame in its place.
    let mut run = Vec::with_capacity(out.len());
    for item in out {
        let member = matches!(
            &item,
            Out::Send {
                message: arena_protocol::ServerMessage::Member(_)
                    | arena_protocol::ServerMessage::MemberLeft { .. },
                ..
            }
        );
        if !member {
            run.push(item);
            continue;
        }
        deliver(app, id, conns, std::mem::take(&mut run));
        let Out::Send { conn, message } = item else {
            continue;
        };
        let Some(sender) = crate::get_conn(conns, conn) else {
            continue;
        };
        let frame = Bytes::from(message.encode());
        Metrics::add(&app.metrics.bytes_out, frame.len() as u64);
        if sender.try_send(Outbound::Frame(frame)).is_err() {
            Metrics::add(&app.metrics.frames_dropped, 1);
            // Dropping the sender closes the connection once it drains.
            remove_conn(conns, conn);
        }
    }
    deliver(app, id, conns, run);
}

fn apply(world: &mut World, conns: &mut Conns, command: WorldCommand) -> Vec<Out> {
    match command {
        WorldCommand::Attach {
            player,
            conn,
            handle,
            tags,
            appearance,
            out,
        } => {
            insert_conn(conns, conn, out);
            world.attach(player, conn, handle, tags, appearance.map(|look| *look))
        }
        WorldCommand::Input { conn, frame } => {
            world.receive(conn, frame);
            Vec::new()
        }
        WorldCommand::Detach { conn } => {
            remove_conn(conns, conn);
            world.detach(conn);
            Vec::new()
        }
        WorldCommand::Directory { blocks, allowed } => {
            let mut out = world.set_allowed(allowed);
            out.extend(world.set_blocks(blocks));
            out
        }
    }
}

async fn run_world(
    app: Arc<App>,
    id: u64,
    mut world: World,
    mut commands: mpsc::Receiver<WorldCommand>,
    report: Arc<Mutex<WorldReport>>,
) {
    let mut conns: Conns = Vec::new();
    let mut interval = tokio::time::interval(app.config.tick);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut shutdown = app.shutdown.subscribe();
    let interval_us = app.config.tick.as_micros() as u64;
    let mut players = 0usize;
    loop {
        tokio::select! {
            biased;
            _ = shutdown.wait_for(|stop| *stop) => {
                let out = world.shut_down();
                deliver_world(&app, id, &mut conns, out);
            }
            _ = interval.tick() => {
                let started = tokio::time::Instant::now();
                let out = world.tick();
                deliver_world(&app, id, &mut conns, out);
                app.metrics.record_tick(started.elapsed().as_micros() as u64, interval_us);
                *report.lock().expect("world report") = world.report();
            }
            command = commands.recv() => {
                let Some(command) = command else { break };
                let out = apply(&mut world, &mut conns, command);
                deliver_world(&app, id, &mut conns, out);
                for _ in 0..64 {
                    match commands.try_recv() {
                        Ok(next) => {
                            let out = apply(&mut world, &mut conns, next);
                            deliver_world(&app, id, &mut conns, out);
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        let now = world.player_count();
        if now != players {
            if now > players {
                Metrics::add(&app.metrics.world_players, (now - players) as u64);
            } else {
                Metrics::sub(&app.metrics.world_players, (players - now) as u64);
            }
            players = now;
        }
        if world.is_ended() {
            break;
        }
    }
    // Leave the registry first, so the next ticket for this id starts a
    // fresh world; anyone whose arrival was already queued is told to come
    // back (ServerShutdown is not final), which reaches the fresh one.
    app.worlds.lock().expect("world registry").live.remove(&id);
    commands.close();
    while let Ok(command) = commands.try_recv() {
        if let WorldCommand::Attach { out, .. } = command {
            let _ = out.try_send(Outbound::Close(Some(ErrorCode::ServerShutdown)));
        }
    }
    let out = world.shut_down();
    deliver_world(&app, id, &mut conns, out);
    Metrics::sub(&app.metrics.world_players, players as u64);
    Metrics::sub(&app.metrics.worlds_active, 1);
    tracing::info!(match_id = %MatchId(id), "hub world ended");
}

/// `POST /worlds/report`: the front door's instance directory exchange.
pub(crate) async fn directory(
    State(app): State<Arc<App>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if body.len() > MAX_DIRECTORY_BODY {
        return StatusCode::PAYLOAD_TOO_LARGE.into_response();
    }
    let signed = headers
        .get(DIRECTORY_SIGNATURE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|tag| app.config.key.verify_signed(DIRECTORY_LABEL, &body, tag));
    if !signed {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let Ok(request) = serde_json::from_slice::<DirectoryRequest>(&body) else {
        return StatusCode::BAD_REQUEST.into_response();
    };
    let now_ms = unix_now().saturating_mul(1000);
    if request.now_ms.abs_diff(now_ms) > DIRECTORY_SKEW_MS + 1000 {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    // Strictly newer than the last accepted request: no replays.
    let last = &app.directory_last;
    let mut seen = last.load(Ordering::Relaxed);
    loop {
        if request.now_ms <= seen {
            return StatusCode::CONFLICT.into_response();
        }
        match last.compare_exchange(seen, request.now_ms, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(current) => seen = current,
        }
    }
    // Kept for worlds started before the next report, and shared by all.
    let blocks = block_set(request.blocks);
    let allowed: Allowed = request
        .players
        .map(|players| Arc::new(players.into_iter().collect()));
    *app.directory_blocks.lock().expect("directory blocks") = (blocks.clone(), allowed.clone());
    for sender in app.world_senders() {
        // A full queue skips this round; the next report repeats the lists.
        let _ = sender.try_send(WorldCommand::Directory {
            blocks: blocks.clone(),
            allowed: allowed.clone(),
        });
    }
    axum::Json(DirectoryReply {
        draining: app.is_draining(),
        worlds: app.world_reports(),
    })
    .into_response()
}

/// How often the front door should ask (it may ask less often).
pub const DIRECTORY_EVERY: Duration = Duration::from_secs(1);
