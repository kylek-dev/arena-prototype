//! Authoritative match server (ADR 0044, `docs/PROTOCOL.md`).
//!
//! Each match is one Tokio task that owns a [`session::Session`] and ticks
//! it at a fixed interval; matches share nothing, so a process scales with
//! its cores and a fleet scales by adding processes. Connections verify a
//! signed join ticket, then only forward intent; the session decides every
//! outcome with the shared rules. A match starts when the first ticket for
//! it arrives (see [`ticket`]); the front door ([`door`]) only signs them.

pub mod anomaly;
pub mod door;
pub mod identity;
pub mod ingress;
pub mod logging;
pub mod matchmaker;
pub mod metrics;
pub mod profiling;
pub mod session;
pub mod ticket;

use std::collections::HashMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arena_protocol::{
    ClientMessage, ErrorCode, InputFrame, PROTOCOL_VERSION, ServerMessage, WireAppearance,
    peek_hello_protocol,
};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::{Arena, MAX_ROUND_TICKS};
use arena_sim::map::Layout;
use axum::Router;
use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::Deserialize;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use anomaly::SharedThrottle;
use ingress::{Cidr, OpenPerAddress, Origins};
use logging::MatchId;
use metrics::Metrics;
use session::{ConnId, EncodedOut, Limits, Out, Session};
use ticket::{Claims, TicketKey};

pub use ticket::NewMatch;

pub struct Config {
    pub bind: SocketAddr,
    /// Enables `POST /dev/match`; refused unless `bind` is loopback.
    pub dev: bool,
    /// Logs every decoded message as JSON on stderr.
    pub debug_json: bool,
    /// Wall-clock length of one rules tick. The rules always count 30 ticks
    /// per second; tests shorten this to run faster.
    pub tick: Duration,
    pub hello_timeout: Duration,
    pub ticket_ttl: Duration,
    /// Messages (inputs and pings) one connection may send per second.
    pub messages_per_second: u32,
    /// Pages whose browsers may connect to `/play`.
    pub origins: Origins,
    /// Reverse proxies whose `X-Forwarded-For` names the client.
    pub trusted_proxies: Vec<Cidr>,
    /// Open connections one client address may hold; 0 is unlimited.
    pub connections_per_address: u32,
    pub limits: Limits,
    /// Total ticks in a round before a time-limit draw (`1..=MAX_ROUND_TICKS`).
    pub round_ticks: u64,
    pub key: TicketKey,
}
impl Config {
    pub fn new(key: TicketKey) -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8080)),
            dev: false,
            debug_json: false,
            tick: Duration::from_secs(1) / TICKS_PER_SECOND as u32,
            hello_timeout: Duration::from_secs(5),
            ticket_ttl: Duration::from_secs(2 * 60 * 60),
            messages_per_second: 2 * TICKS_PER_SECOND as u32,
            origins: Origins::Any,
            trusted_proxies: Vec::new(),
            // A household of six players, each with a spare tab.
            connections_per_address: 12,
            limits: Limits::default(),
            round_ticks: MAX_ROUND_TICKS,
            key,
        }
    }
}

enum Command {
    Attach {
        seat: usize,
        conn: ConnId,
        /// Boxed: looks are large and the other commands are small.
        appearance: Option<Box<WireAppearance>>,
        /// From the ticket; the client never names itself.
        handle: Option<arena_protocol::Handle>,
        out: mpsc::Sender<Outbound>,
    },
    AttachSpectator {
        player: u64,
        conn: ConnId,
        out: mpsc::Sender<Outbound>,
    },
    Input {
        seat: usize,
        conn: ConnId,
        frame: InputFrame,
    },
    Detach {
        seat: usize,
        conn: ConnId,
    },
    DetachSpectator {
        conn: ConnId,
    },
}

enum Outbound {
    Frame(Bytes),
    Close(Option<ErrorCode>),
}

/// Frames buffered per connection before the match drops snapshots for it.
const OUTBOUND_QUEUE: usize = 64;
/// Commands buffered per match; readers wait when it is full.
const COMMAND_QUEUE: usize = 1024;

/// The matches this process hosts, and those it hosted whose tickets are
/// still valid: a rejoin with one must not start the match again.
#[derive(Default)]
struct Registry {
    live: HashMap<u64, mpsc::Sender<Command>>,
    /// Ended match identifiers and when their tickets expire.
    ended: HashMap<u64, u64>,
}

pub struct App {
    config: Config,
    metrics: Arc<Metrics>,
    matches: Mutex<Registry>,
    open: Arc<OpenPerAddress>,
    next_conn: AtomicU64,
    draining: AtomicBool,
    shutdown: watch::Sender<bool>,
    layout: Layout,
    map: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreateError {
    Draining,
    InvalidTeamSize,
}

impl App {
    fn new(config: Config) -> Arc<Self> {
        let map = arena_map::colosseum();
        Arc::new(Self {
            open: OpenPerAddress::new(config.connections_per_address),
            config,
            metrics: Arc::default(),
            matches: Mutex::default(),
            next_conn: AtomicU64::new(1),
            draining: AtomicBool::new(false),
            shutdown: watch::Sender::new(false),
            layout: map.layout,
            map: fnv1a(arena_map::COLOSSEUM_JSON.as_bytes()),
        })
    }
    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }
    pub fn key(&self) -> &TicketKey {
        &self.config.key
    }
    pub fn is_draining(&self) -> bool {
        self.draining.load(Ordering::Relaxed)
    }

    /// Issues a ticket per seat for a new match on this server, which
    /// starts when the first ticket connects (development and tests; in
    /// production the front door issues tickets).
    pub fn issue_match(&self, team_size: usize) -> Result<NewMatch, CreateError> {
        self.issue_match_with_spectators(team_size, 0)
    }

    /// Issues seat tickets plus optional spectator tickets for development
    /// and bot benchmarks.
    pub fn issue_match_with_spectators(
        &self,
        team_size: usize,
        spectators: usize,
    ) -> Result<NewMatch, CreateError> {
        if self.is_draining() {
            return Err(CreateError::Draining);
        }
        let expires = unix_now() + self.config.ticket_ttl.as_secs();
        ticket::issue_match_with_spectators(&self.config.key, team_size, spectators, expires)
            .ok_or(CreateError::InvalidTeamSize)
    }

    /// The match a verified ticket names, starting it if this is the first
    /// ticket to arrive. Must be called inside the Tokio runtime.
    fn match_for(self: &Arc<Self>, claims: &Claims) -> Result<mpsc::Sender<Command>, ErrorCode> {
        if !claims.is_valid() {
            return Err(ErrorCode::BadTicket);
        }
        let mut matches = self.matches.lock().expect("match registry lock");
        if let Some(sender) = matches.live.get(&claims.match_id) {
            return Ok(sender.clone());
        }
        if matches.ended.contains_key(&claims.match_id) {
            return Err(ErrorCode::MatchOver);
        }
        // A draining server starts nothing new. The player keeps retrying
        // and joins once a replacement server is up.
        if self.is_draining() {
            return Err(ErrorCode::ServerShutdown);
        }
        let team_size = usize::from(claims.team_size);
        let (layout, map_hash, round_ticks) = match claims.settings {
            Some(settings) => {
                let (layout, map_hash) = match settings.map {
                    ticket::MapId::Colosseum => (self.layout.clone(), self.map),
                };
                (layout, map_hash, settings.round_ticks())
            }
            None => (self.layout.clone(), self.map, self.config.round_ticks),
        };
        let arena = Arena::practice(layout, team_size)
            .and_then(|arena| arena.with_round_limit(round_ticks))
            .ok_or(ErrorCode::BadTicket)?;
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE);
        matches.live.insert(claims.match_id, sender.clone());
        drop(matches);
        let id = claims.match_id;
        Metrics::add(&self.metrics.matches_created, 1);
        Metrics::add(&self.metrics.matches_active, 1);
        // One span per match, from its first connection to its end. It is
        // never entered while ticking: lifecycle steps are its children.
        let span = crate::control_span!(
            parent: None,
            "match",
            match_id = %MatchId(id),
            team_size,
            result = tracing::field::Empty
        );
        span.in_scope(|| {
            tracing::info!(match_id = %MatchId(id), team_size, "match started by its first connection");
        });
        let session = Session::new(arena, map_hash, self.config.limits, self.metrics.clone());
        tokio::spawn(run_match(
            self.clone(),
            id,
            claims.expires,
            session,
            receiver,
            span,
        ));
        Ok(sender)
    }

    fn end_match(&self, id: u64, expires: u64) {
        let now = unix_now();
        let mut matches = self.matches.lock().expect("match registry lock");
        matches.live.remove(&id);
        matches.ended.retain(|_, expires| *expires > now);
        if expires > now {
            matches.ended.insert(id, expires);
        }
    }
}

/// Seated players and spectators for one match; linear scan avoids hashing on every frame.
type Conns = Vec<(ConnId, mpsc::Sender<Outbound>)>;

fn insert_conn(conns: &mut Conns, conn: ConnId, sender: mpsc::Sender<Outbound>) {
    if let Some((_, existing)) = conns.iter_mut().find(|(id, _)| *id == conn) {
        *existing = sender;
    } else {
        conns.push((conn, sender));
    }
}

fn get_conn(conns: &Conns, conn: ConnId) -> Option<&mpsc::Sender<Outbound>> {
    conns
        .iter()
        .find(|(id, _)| *id == conn)
        .map(|(_, sender)| sender)
}

fn remove_conn(conns: &mut Conns, conn: ConnId) -> Option<mpsc::Sender<Outbound>> {
    let at = conns.iter().position(|(id, _)| *id == conn)?;
    Some(conns.swap_remove(at).1)
}

/// Applies one command; returns the frames dropped for slow clients.
fn apply_command(
    app: &App,
    id: u64,
    session: &mut Session,
    conns: &mut Conns,
    command: Option<Command>,
    span: &tracing::Span,
) -> u64 {
    let out = match command {
        Some(Command::Attach {
            seat,
            conn,
            appearance,
            handle,
            out,
        }) => {
            insert_conn(conns, conn, out);
            let mut out = session.attach_with_appearance(seat, conn, appearance.map(|look| *look));
            log_attach(span, id, seat, conn, &out);
            if welcomed(&out, conn) {
                out.extend(session.relay_handles(seat, conn, handle));
            }
            out
        }
        Some(Command::AttachSpectator { player, conn, out }) => {
            insert_conn(conns, conn, out);
            let mut out = session.attach_spectator(player, conn);
            log_spectator_attach(span, id, conn, &out);
            if welcomed(&out, conn) {
                out.extend(session.handles_for(conn));
            }
            out
        }
        Some(Command::Input { seat, conn, frame }) => {
            session.receive(seat, conn, frame);
            return 0;
        }
        Some(Command::Detach { seat, conn }) => {
            remove_conn(conns, conn);
            if session.holds(seat, conn) {
                crate::control_span!(parent: span, "seat.disconnect", seat, conn_id = conn)
                    .in_scope(|| {
                        tracing::info!(match_id = %MatchId(id), seat, conn_id = conn, "seat disconnected");
                    });
            }
            session.detach(seat, conn);
            return 0;
        }
        Some(Command::DetachSpectator { conn }) => {
            remove_conn(conns, conn);
            session.detach_spectator(conn);
            return 0;
        }
        None => session.shut_down(),
    };
    deliver(app, id, conns, out)
}

async fn run_match(
    app: Arc<App>,
    id: u64,
    expires: u64,
    mut session: Session,
    mut commands: mpsc::Receiver<Command>,
    span: tracing::Span,
) {
    let mut conns: Conns = Vec::with_capacity(arena_protocol::MAX_FIGHTERS);
    // Under overload the clock stretches rather than bursting missed ticks,
    // so the rules never run faster than real time.
    let mut interval = tokio::time::interval(app.config.tick);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut shutdown = app.shutdown.subscribe();
    let interval_us = app.config.tick.as_micros() as u64;
    // Development only: one line per seat every five seconds with the input
    // flow measured on the server's clock, to diagnose pacing.
    let mut report_at = Instant::now() + Duration::from_secs(5);
    let mut report_from = Instant::now();
    loop {
        let mut ticked: Option<(Instant, bool)> = None;
        let mut frames_dropped = 0u64;
        tokio::select! {
            biased;
            _ = shutdown.wait_for(|stop| *stop) => {
                let out = session.shut_down();
                frames_dropped += deliver(&app, id, &mut conns, out);
            }
            _ = interval.tick() => {
                let was_running = session.phase() == session::Phase::Running;
                ticked = Some((Instant::now(), was_running));
                if app.config.debug_json {
                    let out = session.tick();
                    frames_dropped += deliver(&app, id, &mut conns, out);
                } else {
                    let mut bytes_out = 0u64;
                    let mut dropped = 0u64;
                    session.tick_encoded(|out| match out {
                        EncodedOut::Frame { conn, frame } => {
                            let Some(sender) = get_conn(&conns, conn) else {
                                return;
                            };
                            bytes_out += frame.len() as u64;
                            if let Err(mpsc::error::TrySendError::Full(_)) =
                                sender.try_send(Outbound::Frame(Bytes::from(frame)))
                            {
                                dropped += 1;
                            }
                        }
                        EncodedOut::SharedFrame { conn, frame } => {
                            let Some(sender) = get_conn(&conns, conn) else {
                                return;
                            };
                            bytes_out += frame.len() as u64;
                            if let Err(mpsc::error::TrySendError::Full(_)) =
                                sender.try_send(Outbound::Frame(Bytes::from_owner(frame)))
                            {
                                dropped += 1;
                            }
                        }
                        EncodedOut::Close { conn, error } => {
                            if let Some(sender) = remove_conn(&mut conns, conn) {
                                let _ = sender.try_send(Outbound::Close(error));
                            }
                        }
                    });
                    if bytes_out > 0 {
                        Metrics::add(&app.metrics.bytes_out, bytes_out);
                    }
                    if dropped > 0 {
                        Metrics::add(&app.metrics.frames_dropped, dropped);
                        frames_dropped += dropped;
                    }
                }
            }
            command = commands.recv() => {
                frames_dropped += apply_command(&app, id, &mut session, &mut conns, command, &span);
                // Drain a burst of queued inputs without re-entering select!
                // while keeping a small bound so ticks are never starved.
                for _ in 0..64 {
                    if session.is_ended() {
                        break;
                    }
                    match commands.try_recv() {
                        Ok(next) => {
                            frames_dropped +=
                                apply_command(&app, id, &mut session, &mut conns, Some(next), &span);
                        }
                        Err(_) => break,
                    }
                }
            }
        }
        if frames_dropped > 0
            && let Some(suppressed) = DROPS.allow(std::time::Instant::now())
        {
            span.in_scope(|| {
                tracing::warn!(
                    match_id = %MatchId(id),
                    frames_dropped,
                    suppressed_reports = suppressed,
                    reason = "slow_client",
                    "dropped frames for a client whose send queue was full"
                );
            });
        }
        if let Some((started, was_running)) = ticked {
            let elapsed_us = started.elapsed().as_micros() as u64;
            if was_running {
                app.metrics.record_running_tick(elapsed_us, interval_us);
            } else {
                app.metrics.record_tick(elapsed_us, interval_us);
            }
            if elapsed_us > interval_us
                && let Some(suppressed) = OVERRUNS.allow(std::time::Instant::now())
            {
                span.in_scope(|| {
                    tracing::warn!(
                        match_id = %MatchId(id),
                        tick_microseconds = elapsed_us,
                        budget_microseconds = interval_us,
                        suppressed_reports = suppressed,
                        reason = "tick_overrun",
                        "a tick took longer than the tick interval"
                    );
                });
            }
            if app.config.dev && started >= report_at {
                let seconds = started.duration_since(report_from).as_secs_f64();
                for (seat, window) in session.take_windows() {
                    let rate = |count: u32| f64::from(count) / seconds;
                    tracing::info!(
                        match_id = %MatchId(id),
                        seat,
                        window_seconds = seconds,
                        ticks_per_second = rate(window.ticks),
                        inputs_received_per_second = rate(window.received),
                        inputs_applied_per_second = rate(window.applied),
                        folded = window.folded,
                        starved = window.starved,
                        queue_min = window.queue_min.unwrap_or(0),
                        queue_max = window.queue_max,
                        "seat input report"
                    );
                }
                report_from = started;
                report_at = started + Duration::from_secs(5);
            }
        }
        if session.is_ended() {
            break;
        }
    }
    app.end_match(id, expires);
    Metrics::sub(&app.metrics.matches_active, 1);
    let _ended = span.enter();
    match session.arena().result() {
        Some(result) => {
            span.record("result", tracing::field::debug(result));
            tracing::info!(match_id = %MatchId(id), result = ?result, "match ended");
        }
        None => {
            span.record("result", "none");
            tracing::info!(
                match_id = %MatchId(id),
                reason = "no_result",
                "match ended without a result (seats never all joined, everyone left, or the server stopped)"
            );
        }
    }
}

/// Whether `out` welcomes `conn` (its attach succeeded).
fn welcomed(out: &[Out], conn: ConnId) -> bool {
    out.iter().any(|out| {
        matches!(out, Out::Send { conn: to, message: ServerMessage::Welcome(_) } if *to == conn)
    })
}

/// One line per seat change, so an operator (or a puzzled tester) can see
/// who joined which seat of which match. Tickets are never logged.
fn log_attach(span: &tracing::Span, id: u64, seat: usize, conn: ConnId, out: &[Out]) {
    for out in out {
        match out {
            Out::Send {
                conn: to,
                message: ServerMessage::Welcome(_),
            } if *to == conn => {
                crate::control_span!(parent: span, "seat.join", seat, conn_id = conn).in_scope(|| {
                    tracing::info!(match_id = %MatchId(id), seat, conn_id = conn, "seat joined");
                });
            }
            Out::Close {
                conn: refused,
                error: Some(code),
            } if *refused == conn => {
                crate::control_span!(parent: span, "seat.refuse", seat, conn_id = conn).in_scope(
                    || {
                        tracing::info!(
                            match_id = %MatchId(id),
                            seat,
                            conn_id = conn,
                            error_code = ?code,
                            "seat refused connection"
                        );
                    },
                );
            }
            Out::Close {
                conn: old,
                error: Some(ErrorCode::Replaced),
            } => {
                crate::control_span!(parent: span, "seat.replace", seat, conn_id = conn).in_scope(
                    || {
                        tracing::info!(
                            match_id = %MatchId(id),
                            seat,
                            conn_id = conn,
                            replaced_conn_id = *old,
                            reason = "replaced",
                            "seat taken over by a new connection"
                        );
                    },
                );
            }
            _ => {}
        }
    }
}

fn log_spectator_attach(span: &tracing::Span, id: u64, conn: ConnId, out: &[Out]) {
    for out in out {
        match out {
            Out::Send {
                conn: to,
                message: ServerMessage::Welcome(_),
            } if *to == conn => {
                crate::control_span!(parent: span, "spectator.join", conn_id = conn).in_scope(
                    || {
                        tracing::info!(match_id = %MatchId(id), conn_id = conn, "spectator joined");
                    },
                );
            }
            Out::Close {
                conn: refused,
                error: Some(code),
            } if *refused == conn => {
                crate::control_span!(parent: span, "spectator.refuse", conn_id = conn).in_scope(
                    || {
                        tracing::info!(
                            match_id = %MatchId(id),
                            conn_id = conn,
                            error_code = ?code,
                            "spectator refused connection"
                        );
                    },
                );
            }
            _ => {}
        }
    }
}

/// Sends a match's output; returns the frames dropped for slow clients.
fn deliver(app: &App, id: u64, conns: &mut Conns, out: Vec<Out>) -> u64 {
    let mut bytes_out = 0u64;
    let mut frames_dropped = 0u64;
    for out in out {
        match out {
            Out::Send { conn, message } => {
                let Some(sender) = get_conn(conns, conn) else {
                    continue;
                };
                if app.config.debug_json {
                    tracing::info!(
                        target: "arena_server::wire",
                        match_id = %MatchId(id),
                        conn_id = conn,
                        payload = %message.to_debug_json(),
                        "to client"
                    );
                }
                let frame = message.encode();
                bytes_out += frame.len() as u64;
                if let Err(mpsc::error::TrySendError::Full(_)) =
                    sender.try_send(Outbound::Frame(Bytes::from(frame)))
                {
                    frames_dropped += 1;
                }
            }
            Out::Close { conn, error } => {
                // Dropping the sender also closes a connection whose queue is
                // full, after it drains what is already queued.
                if let Some(sender) = remove_conn(conns, conn) {
                    let _ = sender.try_send(Outbound::Close(error));
                }
            }
        }
    }
    if bytes_out > 0 {
        Metrics::add(&app.metrics.bytes_out, bytes_out);
    }
    if frames_dropped > 0 {
        Metrics::add(&app.metrics.frames_dropped, frames_dropped);
    }
    frames_dropped
}

pub fn router(app: Arc<App>) -> Router {
    let mut router = Router::new()
        .route("/play", get(play))
        .route("/healthz", get(|| async { "ok" }))
        .route("/readyz", get(ready))
        .route("/metrics", get(render_metrics));
    if app.config.dev {
        router = router.route("/dev/match", post(dev_match));
    }
    traced(router.with_state(app))
}

/// Paths scraped or probed every few seconds: no span each time.
const UNTRACED_PATHS: [&str; 3] = ["/metrics", "/healthz", "/readyz"];

/// One `http.request` span per request (a WebSocket's is its upgrade), with
/// the method, the route (never the query; `unmatched` for unknown paths, so
/// probes cannot mint arbitrary values) and the status. Probes and scrapes
/// get none. Failures are not logged here: callers already count them.
pub fn traced(router: Router) -> Router {
    router.layer(
        tower_http::trace::TraceLayer::new_for_http()
            .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                let path = request
                    .extensions()
                    .get::<axum::extract::MatchedPath>()
                    .map_or("unmatched", axum::extract::MatchedPath::as_str);
                if UNTRACED_PATHS.contains(&path) {
                    return tracing::Span::none();
                }
                crate::control_span!(
                    "http.request",
                    method = %request.method(),
                    path = %path,
                    status = tracing::field::Empty
                )
            })
            .on_failure(tower_http::trace::DefaultOnFailure::new().level(tracing::Level::DEBUG))
            .on_response(
                |response: &axum::http::Response<axum::body::Body>,
                 _latency: Duration,
                 span: &tracing::Span| {
                    span.record("status", response.status().as_u16());
                },
            ),
    )
}

async fn ready(State(app): State<Arc<App>>) -> Response {
    if app.is_draining() {
        (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response()
    } else {
        "ready".into_response()
    }
}

async fn render_metrics(State(app): State<Arc<App>>) -> String {
    let mut text = app.metrics.render();
    let _ = std::fmt::Write::write_fmt(
        &mut text,
        format_args!(
            "# HELP arena_draining 1 while the server drains after a stop signal.\n# TYPE arena_draining gauge\narena_draining {}\n",
            u8::from(app.is_draining())
        ),
    );
    metrics::ProcessStats::read().render(&mut text);
    metrics::render_runtime(&mut text);
    text
}

#[derive(Deserialize)]
struct DevMatchQuery {
    team_size: Option<usize>,
    spectators: Option<usize>,
}

async fn dev_match(State(app): State<Arc<App>>, Query(query): Query<DevMatchQuery>) -> Response {
    let spectators = query.spectators.unwrap_or(0);
    if spectators > 64 {
        return (StatusCode::BAD_REQUEST, "spectators must be at most 64").into_response();
    }
    match app.issue_match_with_spectators(query.team_size.unwrap_or(1), spectators) {
        Ok(created) => axum::Json(created).into_response(),
        Err(CreateError::Draining) => (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response(),
        Err(CreateError::InvalidTeamSize) => {
            (StatusCode::BAD_REQUEST, "team_size must be 1, 2 or 3").into_response()
        }
    }
}

async fn play(
    ws: WebSocketUpgrade,
    State(app): State<Arc<App>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
) -> Response {
    if !app.config.origins.allows(&headers) {
        Metrics::add(&app.metrics.refused_origin, 1);
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let address: IpAddr = ingress::client_address(peer, &headers, &app.config.trusted_proxies);
    let Some(slot) = app.open.open(address) else {
        Metrics::add(&app.metrics.refused_address, 1);
        return (
            StatusCode::TOO_MANY_REQUESTS,
            "too many connections from this address",
        )
            .into_response();
    };
    ws.max_message_size(arena_protocol::MAX_CLIENT_FRAME)
        .max_frame_size(arena_protocol::MAX_CLIENT_FRAME)
        .on_upgrade(move |mut socket| async move {
            connection(&mut socket, app).await;
            // The address keeps its slot until the socket is really gone.
            linger(socket).await;
            drop(slot);
        })
}

/// How long a finished connection keeps reading while it closes.
const CLOSE_LINGER: Duration = Duration::from_secs(1);

/// Completes the WebSocket close handshake before the TCP socket is dropped.
///
/// Sends Close (an error if one was already sent, which is ignored), then
/// reads and discards whatever the peer had already sent until it answers
/// the Close, hangs up, or [`CLOSE_LINGER`] passes. Dropping a TCP socket
/// with unread data makes the kernel reset the connection instead of
/// closing it, and a reset can discard our final `Error` frame before the
/// peer reads it, so a flooding client would never learn why it was cut off.
pub(crate) async fn linger(mut socket: WebSocket) {
    let _ = socket.send(Message::Close(None)).await;
    let _ = tokio::time::timeout(CLOSE_LINGER, async {
        while let Some(Ok(_)) = socket.recv().await {}
    })
    .await;
}

/// Fixed one-second message budget for a connection.
#[derive(Debug)]
pub struct RateWindow {
    started: Instant,
    count: u32,
    limit: u32,
}
impl RateWindow {
    pub fn new(limit: u32, now: Instant) -> Self {
        Self {
            started: now,
            count: 0,
            limit,
        }
    }
    /// Counts one message; `false` once the window's budget is exceeded.
    pub fn allow(&mut self, now: Instant) -> bool {
        if now.saturating_duration_since(self.started) >= Duration::from_secs(1) {
            self.started = now;
            self.count = 0;
        }
        self.count = self.count.saturating_add(1);
        self.count <= self.limit
    }
}

/// Validates the first frame as a current-version `Hello` and returns its
/// ticket bytes and optional appearance choice.
pub fn read_hello(frame: &[u8]) -> Result<(Vec<u8>, Option<WireAppearance>), ErrorCode> {
    match ClientMessage::decode(frame) {
        Ok(ClientMessage::Hello {
            protocol,
            ticket,
            appearance,
        }) if protocol == PROTOCOL_VERSION => Ok((ticket, appearance.map(|look| *look))),
        Ok(ClientMessage::Hello { .. }) => Err(ErrorCode::VersionMismatch),
        Ok(_) => Err(ErrorCode::Malformed),
        Err(_) => match peek_hello_protocol(frame) {
            Some(protocol) if protocol != PROTOCOL_VERSION => Err(ErrorCode::VersionMismatch),
            _ => Err(ErrorCode::Malformed),
        },
    }
}

struct OpenConnection<'a>(&'a Metrics);
impl Drop for OpenConnection<'_> {
    fn drop(&mut self) {
        Metrics::sub(&self.0.connections_open, 1);
    }
}

struct OpenSpectator<'a>(&'a Metrics);
impl Drop for OpenSpectator<'_> {
    fn drop(&mut self) {
        Metrics::sub(&self.0.spectators_open, 1);
    }
}

/// Refusals worth an operator's attention, by kind: bad tickets, floods
/// and garbage. Each is reported at most once per window process-wide.
static REFUSALS: [SharedThrottle; 3] = [
    SharedThrottle::new(anomaly::REPORT_EVERY),
    SharedThrottle::new(anomaly::REPORT_EVERY),
    SharedThrottle::new(anomaly::REPORT_EVERY),
];
/// Tick overruns and dropped frames, process-wide: an overloaded host with
/// hundreds of matches still writes one line per window.
static OVERRUNS: SharedThrottle = SharedThrottle::new(anomaly::REPORT_EVERY);
static DROPS: SharedThrottle = SharedThrottle::new(anomaly::REPORT_EVERY);

async fn reject(socket: &mut WebSocket, app: &App, code: ErrorCode) {
    app.metrics.record_error(code);
    let kind = match code {
        ErrorCode::BadTicket => Some(0),
        ErrorCode::RateLimited => Some(1),
        ErrorCode::Malformed => Some(2),
        _ => None,
    };
    if let Some(kind) = kind
        && let Some(suppressed) = REFUSALS[kind].allow(std::time::Instant::now())
    {
        tracing::warn!(
            error_code = ?code,
            suppressed_reports = suppressed,
            reason = "refused",
            "closed a connection for a bad ticket, malformed frames or flooding"
        );
    }
    let frame = ServerMessage::Error(code).encode();
    let _ = socket.send(Message::Binary(frame.into())).await;
    let _ = socket.send(Message::Close(None)).await;
}

/// Sends one message from the match; `false` once the connection is done.
async fn forward(socket: &mut WebSocket, app: &App, outgoing: Outbound) -> bool {
    match outgoing {
        Outbound::Frame(frame) => socket.send(Message::Binary(frame)).await.is_ok(),
        Outbound::Close(Some(code)) => {
            reject(socket, app, code).await;
            false
        }
        Outbound::Close(None) => {
            let _ = socket.send(Message::Close(None)).await;
            false
        }
    }
}

async fn connection(socket: &mut WebSocket, app: Arc<App>) {
    Metrics::add(&app.metrics.connections_open, 1);
    let _open = OpenConnection(&app.metrics);
    let conn = app.next_conn.fetch_add(1, Ordering::Relaxed);
    let mut rate = RateWindow::new(app.config.messages_per_second, Instant::now());

    let first = tokio::time::timeout(app.config.hello_timeout, async {
        loop {
            match socket.recv().await {
                Some(Ok(Message::Binary(frame))) => return Some(Ok(frame)),
                Some(Ok(Message::Text(_))) => return Some(Err(ErrorCode::Malformed)),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                    if !rate.allow(Instant::now()) {
                        return Some(Err(ErrorCode::RateLimited));
                    }
                    continue;
                }
                _ => return None,
            }
        }
    })
    .await;
    let frame = match first {
        Err(_) => return reject(socket, &app, ErrorCode::HelloTimeout).await,
        Ok(None) => return,
        Ok(Some(Err(code))) => return reject(socket, &app, code).await,
        Ok(Some(Ok(frame))) => frame,
    };
    // The Hello, its ticket check and finding the match: one span per
    // connection, closed before the connection starts playing.
    let hello_span = crate::control_span!(
        "ws.hello",
        conn_id = conn,
        match_id = tracing::field::Empty,
        role = tracing::field::Empty,
        outcome = tracing::field::Empty
    );
    let admitted = hello_span.in_scope(|| -> Result<_, ErrorCode> {
        let (ticket, appearance) = read_hello(&frame)?;
        let claims = app
            .config
            .key
            .verify(&ticket, unix_now())
            .map_err(|_| ErrorCode::BadTicket)?;
        let span = tracing::Span::current();
        span.record(
            "match_id",
            tracing::field::display(MatchId(claims.match_id)),
        );
        span.record("role", tracing::field::debug(claims.role));
        if app.config.debug_json {
            tracing::info!(
                target: "arena_server::wire",
                match_id = %MatchId(claims.match_id),
                conn_id = conn,
                role = ?claims.role,
                "hello"
            );
        }
        let commands = app.match_for(&claims)?;
        Ok((claims, appearance, commands))
    });
    let (claims, appearance, commands) = match admitted {
        Ok(admitted) => {
            hello_span.record("outcome", "admitted");
            admitted
        }
        Err(code) => {
            hello_span.record("outcome", tracing::field::debug(code));
            drop(hello_span);
            return reject(socket, &app, code).await;
        }
    };
    drop(hello_span);
    let (out_sender, mut outbound) = mpsc::channel(OUTBOUND_QUEUE);
    let _spec_open = if claims.is_spectator() {
        Metrics::add(&app.metrics.spectators_open, 1);
        Some(OpenSpectator(&app.metrics))
    } else {
        None
    };
    let attach = match claims.role {
        ticket::Role::Player { seat } => Command::Attach {
            seat: usize::from(seat),
            conn,
            appearance: appearance.map(Box::new),
            handle: claims.handle,
            out: out_sender,
        },
        ticket::Role::Spectator => Command::AttachSpectator {
            player: claims.player,
            conn,
            out: out_sender,
        },
    };
    if commands.send(attach).await.is_err() {
        return reject(socket, &app, ErrorCode::MatchOver).await;
    }

    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(frame))) => frame,
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => {
                        if !rate.allow(Instant::now()) {
                            reject(socket, &app, ErrorCode::RateLimited).await;
                            break;
                        }
                        continue;
                    }
                    Some(Ok(Message::Text(_))) => {
                        reject(socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                    // Closed, or a transport error such as an oversized frame.
                    _ => break,
                };
                if !rate.allow(Instant::now()) {
                    reject(socket, &app, ErrorCode::RateLimited).await;
                    break;
                }
                let message = match ClientMessage::decode(&frame) {
                    Ok(message) => message,
                    Err(_) => {
                        reject(socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                };
                if app.config.debug_json {
                    // A repeated Hello carries a ticket: never print it.
                    let shown = match &message {
                        ClientMessage::Hello {
                            protocol,
                            appearance,
                            ..
                        } => ClientMessage::Hello {
                            protocol: *protocol,
                            ticket: Vec::new(),
                            appearance: appearance.clone(),
                        }
                        .to_debug_json()
                        .replacen("\"ticket\":[]", "\"ticket\":\"<redacted>\"", 1),
                        other => other.to_debug_json(),
                    };
                    tracing::info!(
                        target: "arena_server::wire",
                        conn_id = conn,
                        payload = %shown,
                        "from client"
                    );
                }
                match message {
                    ClientMessage::Input(frame) => {
                        // Watch-only spectators never act: inputs are strictly ignored.
                        let Some(seat) = claims.seat() else {
                            continue;
                        };
                        if commands.send(Command::Input { seat, conn, frame }).await.is_err() {
                            // The match task has ended; it queued its last
                            // word (usually `MatchOver`) before exiting.
                            while let Some(outgoing) = outbound.recv().await {
                                if !forward(socket, &app, outgoing).await {
                                    break;
                                }
                            }
                            break;
                        }
                    }
                    ClientMessage::Ping { nonce } => {
                        let pong = ServerMessage::Pong { nonce }.encode();
                        if socket.send(Message::Binary(pong.into())).await.is_err() {
                            break;
                        }
                    }
                    ClientMessage::Hello { .. } => {
                        reject(socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                }
            }
            outgoing = outbound.recv() => match outgoing {
                Some(outgoing) => {
                    if !forward(socket, &app, outgoing).await {
                        break;
                    }
                }
                None => {
                    let _ = socket.send(Message::Close(None)).await;
                    break;
                }
            },
        }
    }
    let detach = match claims.role {
        ticket::Role::Player { seat } => Command::Detach {
            seat: usize::from(seat),
            conn,
        },
        ticket::Role::Spectator => Command::DetachSpectator { conn },
    };
    let _ = commands.send(detach).await;
}

/// A started server. Dropping it leaves the server running until the
/// runtime stops; call [`Running::shut_down`] to stop it cleanly.
pub struct Running {
    pub addr: SocketAddr,
    pub app: Arc<App>,
    task: JoinHandle<io::Result<()>>,
}

pub async fn start(config: Config) -> io::Result<Running> {
    if config.dev && !config.bind.ip().is_loopback() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "development match creation requires a loopback bind address",
        ));
    }
    if config.round_ticks == 0 || config.round_ticks > MAX_ROUND_TICKS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("round_ticks must be between 1 and {MAX_ROUND_TICKS}"),
        ));
    }
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let addr = listener.local_addr()?;
    // Snapshots and pongs are small and frequent; Nagle's algorithm would
    // hold them back waiting for acknowledgements (tens of milliseconds).
    let listener = axum::serve::ListenerExt::tap_io(listener, |stream| {
        let _ = stream.set_nodelay(true);
    });
    let app = App::new(config);
    let mut stop = app.shutdown.subscribe();
    let service = router(app.clone()).into_make_service_with_connect_info::<SocketAddr>();
    let server = axum::serve(listener, service).with_graceful_shutdown(async move {
        let _ = stop.wait_for(|stop| *stop).await;
    });
    let task = tokio::spawn(async move { server.await });
    Ok(Running { addr, app, task })
}

impl Running {
    /// Starts draining: readiness fails and no new match starts, while the
    /// matches already running continue (and their players may rejoin).
    pub fn begin_drain(&self) {
        self.app.draining.store(true, Ordering::Relaxed);
    }

    /// Matches still running.
    pub fn active_matches(&self) -> u64 {
        Metrics::get(&self.app.metrics.matches_active)
    }

    /// Waits until every match has ended or `limit` has passed; `true` if
    /// they all ended.
    pub async fn drained(&self, limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        while self.active_matches() > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        self.active_matches() == 0
    }

    /// Refuses new matches, lets current ones finish for up to `drain`, then
    /// closes the rest with `ServerShutdown` and stops listening.
    pub async fn shut_down(self, drain: Duration) -> io::Result<()> {
        self.begin_drain();
        self.drained(drain).await;
        self.stop().await
    }

    /// Closes every remaining match with `ServerShutdown` and stops
    /// listening, without waiting for matches to end.
    pub async fn stop(self) -> io::Result<()> {
        self.begin_drain();
        let metrics = &self.app.metrics;
        self.app.shutdown.send_replace(true);
        // Give connections a moment to deliver their final message.
        let deadline = Instant::now() + Duration::from_secs(2);
        while (Metrics::get(&metrics.matches_active) > 0
            || Metrics::get(&metrics.connections_open) > 0)
            && Instant::now() < deadline
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        self.task.await.map_err(io::Error::other)?
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

/// 64-bit FNV-1a, used to identify the map a match runs on.
pub fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, &byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_frames_resolve_to_a_ticket_or_the_right_error() {
        let hello = |protocol| {
            ClientMessage::Hello {
                protocol,
                ticket: vec![1, 2],
                appearance: None,
            }
            .encode()
        };
        assert_eq!(read_hello(&hello(PROTOCOL_VERSION)), Ok((vec![1, 2], None)));
        assert_eq!(
            read_hello(&hello(PROTOCOL_VERSION + 1)),
            Err(ErrorCode::VersionMismatch)
        );
        // A future Hello this build cannot even decode still names its version.
        assert_eq!(
            read_hello(&[0, (PROTOCOL_VERSION + 1) as u8, 0xFF, 0xFF]),
            Err(ErrorCode::VersionMismatch)
        );
        assert_eq!(
            read_hello(&ClientMessage::Ping { nonce: 1 }.encode()),
            Err(ErrorCode::Malformed)
        );
        assert_eq!(read_hello(&[]), Err(ErrorCode::Malformed));
        // An older protocol version is reported as VersionMismatch, not Malformed.
        assert_eq!(read_hello(&[0, 1, 9]), Err(ErrorCode::VersionMismatch));
        assert_eq!(
            read_hello(&[0, PROTOCOL_VERSION as u8, 9]),
            Err(ErrorCode::Malformed)
        );
    }

    #[test]
    fn rate_windows_allow_the_budget_per_second_then_refuse() {
        let start = Instant::now();
        let mut window = RateWindow::new(3, start);
        assert!((0..3).all(|_| window.allow(start)));
        assert!(!window.allow(start + Duration::from_millis(999)));
        // Repeated rejections stay refused without overflow, and timestamps
        // earlier than `started` saturate cleanly within the current window.
        assert!(!window.allow(start));
        assert!(window.allow(start + Duration::from_secs(1)));
        assert!(window.allow(start));
        let mut zero = RateWindow::new(0, start);
        assert!(!zero.allow(start));
    }

    #[test]
    fn fnv_matches_the_reference_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
