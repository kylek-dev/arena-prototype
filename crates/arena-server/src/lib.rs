//! Authoritative match server (ADR 0044, `docs/PROTOCOL.md`).
//!
//! Each match is one Tokio task that owns a [`session::Session`] and ticks
//! it at a fixed interval; matches share nothing, so a process scales with
//! its cores and a fleet scales by adding processes. Connections verify a
//! signed join ticket, then only forward intent; the session decides every
//! outcome with the shared rules.

pub mod metrics;
pub mod session;
pub mod ticket;

use std::collections::HashMap;
use std::io;
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use arena_protocol::{
    ClientMessage, ErrorCode, InputFrame, PROTOCOL_VERSION, ServerMessage, peek_hello_protocol,
};
use arena_sim::TICKS_PER_SECOND;
use arena_sim::arena::Arena;
use arena_sim::map::Layout;
use axum::Router;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::{Instant, MissedTickBehavior};

use metrics::Metrics;
use session::{ConnId, Limits, Out, Session};
use ticket::{Claims, TicketKey};

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
    pub limits: Limits,
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
            limits: Limits::default(),
            key,
        }
    }
}

enum Command {
    Attach {
        seat: usize,
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
}

enum Outbound {
    Frame(Vec<u8>),
    Close(Option<ErrorCode>),
}

/// Frames buffered per connection before the match drops snapshots for it.
const OUTBOUND_QUEUE: usize = 64;
/// Commands buffered per match; readers wait when it is full.
const COMMAND_QUEUE: usize = 1024;

pub struct App {
    config: Config,
    metrics: Arc<Metrics>,
    matches: Mutex<HashMap<u64, mpsc::Sender<Command>>>,
    next_conn: AtomicU64,
    draining: AtomicBool,
    shutdown: watch::Sender<bool>,
    layout: Layout,
    map: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewMatch {
    pub match_id: u64,
    pub team_size: usize,
    /// Hex-encoded join tickets, one per seat in actor order.
    pub tickets: Vec<String>,
    /// Unix seconds after which the tickets are refused.
    pub expires: u64,
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

    /// Starts a match on the default map and issues a ticket per seat.
    /// Must be called inside the Tokio runtime.
    pub fn create_match(self: &Arc<Self>, team_size: usize) -> Result<NewMatch, CreateError> {
        if self.is_draining() {
            return Err(CreateError::Draining);
        }
        let arena =
            Arena::practice(self.layout.clone(), team_size).ok_or(CreateError::InvalidTeamSize)?;
        let seats = arena.fighters().len();
        let (sender, receiver) = mpsc::channel(COMMAND_QUEUE);
        let match_id = {
            let mut matches = self.matches.lock().expect("match registry lock");
            let mut id = random_u64();
            while matches.contains_key(&id) {
                id = random_u64();
            }
            matches.insert(id, sender);
            id
        };
        Metrics::add(&self.metrics.matches_created, 1);
        eprintln!("match {match_id:016x}: created for {team_size}v{team_size}");
        Metrics::add(&self.metrics.matches_active, 1);
        let session = Session::new(arena, self.map, self.config.limits, self.metrics.clone());
        tokio::spawn(run_match(self.clone(), match_id, session, receiver));
        let expires = unix_now() + self.config.ticket_ttl.as_secs();
        let tickets = (0..seats)
            .map(|seat| {
                ticket::to_hex(&self.config.key.issue(&Claims {
                    match_id,
                    player: random_u64(),
                    seat: seat as u8,
                    expires,
                }))
            })
            .collect();
        Ok(NewMatch {
            match_id,
            team_size,
            tickets,
            expires,
        })
    }

    fn match_sender(&self, id: u64) -> Option<mpsc::Sender<Command>> {
        self.matches
            .lock()
            .expect("match registry lock")
            .get(&id)
            .cloned()
    }
}

async fn run_match(
    app: Arc<App>,
    id: u64,
    mut session: Session,
    mut commands: mpsc::Receiver<Command>,
) {
    let mut conns: HashMap<ConnId, mpsc::Sender<Outbound>> = HashMap::new();
    // Under overload the clock stretches rather than bursting missed ticks,
    // so the rules never run faster than real time.
    let mut interval = tokio::time::interval(app.config.tick);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    let mut shutdown = app.shutdown.subscribe();
    let interval_us = app.config.tick.as_micros() as u64;
    loop {
        let mut ticked = None;
        let out = tokio::select! {
            biased;
            _ = shutdown.wait_for(|stop| *stop) => session.shut_down(),
            _ = interval.tick() => {
                ticked = Some(Instant::now());
                session.tick()
            }
            command = commands.recv() => match command {
                Some(Command::Attach { seat, conn, out }) => {
                    conns.insert(conn, out);
                    let out = session.attach(seat, conn);
                    log_attach(id, seat, conn, &out);
                    out
                }
                Some(Command::Input { seat, conn, frame }) => {
                    session.receive(seat, conn, frame);
                    Vec::new()
                }
                Some(Command::Detach { seat, conn }) => {
                    conns.remove(&conn);
                    if session.holds(seat, conn) {
                        eprintln!("match {id:016x}: seat {seat} disconnected (connection {conn})");
                    }
                    session.detach(seat, conn);
                    Vec::new()
                }
                None => session.shut_down(),
            },
        };
        deliver(&app, id, &mut conns, out);
        if let Some(started) = ticked {
            app.metrics
                .record_tick(started.elapsed().as_micros() as u64, interval_us);
        }
        if session.is_ended() {
            break;
        }
    }
    app.matches.lock().expect("match registry lock").remove(&id);
    Metrics::sub(&app.metrics.matches_active, 1);
    match session.arena().result() {
        Some(result) => eprintln!("match {id:016x}: ended ({result:?})"),
        None => eprintln!(
            "match {id:016x}: ended without a result (seats never all joined, everyone left, or the server stopped)"
        ),
    }
}

/// One line per seat change, so an operator (or a puzzled tester) can see
/// who joined which seat of which match. Tickets are never logged.
fn log_attach(id: u64, seat: usize, conn: ConnId, out: &[Out]) {
    for out in out {
        match out {
            Out::Send {
                conn: to,
                message: ServerMessage::Welcome(_),
            } if *to == conn => {
                eprintln!("match {id:016x}: seat {seat} joined (connection {conn})");
            }
            Out::Close {
                conn: refused,
                error: Some(code),
            } if *refused == conn => {
                eprintln!("match {id:016x}: seat {seat} refused connection {conn}: {code:?}");
            }
            Out::Close {
                conn: old,
                error: Some(ErrorCode::Replaced),
            } => {
                eprintln!(
                    "match {id:016x}: seat {seat} taken over by connection {conn} from connection {old}"
                );
            }
            _ => {}
        }
    }
}

fn deliver(app: &App, id: u64, conns: &mut HashMap<ConnId, mpsc::Sender<Outbound>>, out: Vec<Out>) {
    for out in out {
        match out {
            Out::Send { conn, message } => {
                let Some(sender) = conns.get(&conn) else {
                    continue;
                };
                if app.config.debug_json {
                    eprintln!(
                        "match {id:016x} -> conn {conn}: {}",
                        message.to_debug_json()
                    );
                }
                let frame = message.encode();
                Metrics::add(&app.metrics.bytes_out, frame.len() as u64);
                if let Err(mpsc::error::TrySendError::Full(_)) =
                    sender.try_send(Outbound::Frame(frame))
                {
                    Metrics::add(&app.metrics.frames_dropped, 1);
                }
            }
            Out::Close { conn, error } => {
                // Dropping the sender also closes a connection whose queue is
                // full, after it drains what is already queued.
                if let Some(sender) = conns.remove(&conn) {
                    let _ = sender.try_send(Outbound::Close(error));
                }
            }
        }
    }
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
    router.with_state(app)
}

async fn ready(State(app): State<Arc<App>>) -> Response {
    if app.is_draining() {
        (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response()
    } else {
        "ready".into_response()
    }
}

async fn render_metrics(State(app): State<Arc<App>>) -> String {
    app.metrics.render()
}

#[derive(Deserialize)]
struct DevMatchQuery {
    team_size: Option<usize>,
}

async fn dev_match(State(app): State<Arc<App>>, Query(query): Query<DevMatchQuery>) -> Response {
    match app.create_match(query.team_size.unwrap_or(1)) {
        Ok(created) => axum::Json(created).into_response(),
        Err(CreateError::Draining) => (StatusCode::SERVICE_UNAVAILABLE, "draining").into_response(),
        Err(CreateError::InvalidTeamSize) => {
            (StatusCode::BAD_REQUEST, "team_size must be 1, 2 or 3").into_response()
        }
    }
}

async fn play(ws: WebSocketUpgrade, State(app): State<Arc<App>>) -> Response {
    ws.max_message_size(arena_protocol::MAX_CLIENT_FRAME)
        .max_frame_size(arena_protocol::MAX_CLIENT_FRAME)
        .on_upgrade(move |socket| connection(socket, app))
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
        if now.duration_since(self.started) >= Duration::from_secs(1) {
            self.started = now;
            self.count = 0;
        }
        self.count += 1;
        self.count <= self.limit
    }
}

/// Validates the first frame as a current-version `Hello` and returns its
/// ticket bytes.
pub fn read_hello(frame: &[u8]) -> Result<Vec<u8>, ErrorCode> {
    match ClientMessage::decode(frame) {
        Ok(ClientMessage::Hello { protocol, ticket }) if protocol == PROTOCOL_VERSION => Ok(ticket),
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

async fn reject(socket: &mut WebSocket, app: &App, code: ErrorCode) {
    app.metrics.record_error(code);
    let frame = ServerMessage::Error(code).encode();
    let _ = socket.send(Message::Binary(frame.into())).await;
    let _ = socket.send(Message::Close(None)).await;
}

/// Sends one message from the match; `false` once the connection is done.
async fn forward(socket: &mut WebSocket, app: &App, outgoing: Outbound) -> bool {
    match outgoing {
        Outbound::Frame(frame) => socket.send(Message::Binary(frame.into())).await.is_ok(),
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

async fn connection(mut socket: WebSocket, app: Arc<App>) {
    Metrics::add(&app.metrics.connections_open, 1);
    let _open = OpenConnection(&app.metrics);
    let conn = app.next_conn.fetch_add(1, Ordering::Relaxed);

    let first = tokio::time::timeout(app.config.hello_timeout, async {
        loop {
            match socket.recv().await {
                Some(Ok(Message::Binary(frame))) => return Some(Ok(frame)),
                Some(Ok(Message::Text(_))) => return Some(Err(ErrorCode::Malformed)),
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                _ => return None,
            }
        }
    })
    .await;
    let frame = match first {
        Err(_) => return reject(&mut socket, &app, ErrorCode::HelloTimeout).await,
        Ok(None) => return,
        Ok(Some(Err(code))) => return reject(&mut socket, &app, code).await,
        Ok(Some(Ok(frame))) => frame,
    };
    let ticket = match read_hello(&frame) {
        Ok(ticket) => ticket,
        Err(code) => return reject(&mut socket, &app, code).await,
    };
    let claims = match app.config.key.verify(&ticket, unix_now()) {
        Ok(claims) => claims,
        Err(_) => return reject(&mut socket, &app, ErrorCode::BadTicket).await,
    };
    if app.config.debug_json {
        eprintln!(
            "conn {conn} hello: match {:016x} seat {}",
            claims.match_id, claims.seat
        );
    }
    // A validly signed ticket for a match that is gone: it has ended.
    let Some(commands) = app.match_sender(claims.match_id) else {
        return reject(&mut socket, &app, ErrorCode::MatchOver).await;
    };
    let seat = usize::from(claims.seat);
    let (out_sender, mut outbound) = mpsc::channel(OUTBOUND_QUEUE);
    if commands
        .send(Command::Attach {
            seat,
            conn,
            out: out_sender,
        })
        .await
        .is_err()
    {
        return reject(&mut socket, &app, ErrorCode::MatchOver).await;
    }

    let mut rate = RateWindow::new(app.config.messages_per_second, Instant::now());
    loop {
        tokio::select! {
            incoming = socket.recv() => {
                let frame = match incoming {
                    Some(Ok(Message::Binary(frame))) => frame,
                    Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                    Some(Ok(Message::Text(_))) => {
                        reject(&mut socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                    // Closed, or a transport error such as an oversized frame.
                    _ => break,
                };
                if !rate.allow(Instant::now()) {
                    reject(&mut socket, &app, ErrorCode::RateLimited).await;
                    break;
                }
                let message = match ClientMessage::decode(&frame) {
                    Ok(message) => message,
                    Err(_) => {
                        reject(&mut socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                };
                if app.config.debug_json {
                    eprintln!("conn {conn} -> match: {}", message.to_debug_json());
                }
                match message {
                    ClientMessage::Input(frame) => {
                        if commands.send(Command::Input { seat, conn, frame }).await.is_err() {
                            // The match task has ended; it queued its last
                            // word (usually `MatchOver`) before exiting.
                            while let Some(outgoing) = outbound.recv().await {
                                if !forward(&mut socket, &app, outgoing).await {
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
                        reject(&mut socket, &app, ErrorCode::Malformed).await;
                        break;
                    }
                }
            }
            outgoing = outbound.recv() => match outgoing {
                Some(outgoing) => {
                    if !forward(&mut socket, &app, outgoing).await {
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
    let _ = commands.send(Command::Detach { seat, conn }).await;
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
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let addr = listener.local_addr()?;
    let app = App::new(config);
    let mut stop = app.shutdown.subscribe();
    let server = axum::serve(listener, router(app.clone())).with_graceful_shutdown(async move {
        let _ = stop.wait_for(|stop| *stop).await;
    });
    let task = tokio::spawn(async move { server.await });
    Ok(Running { addr, app, task })
}

impl Running {
    /// Refuses new matches, lets current ones finish for up to `drain`, then
    /// closes the rest with `ServerShutdown` and stops listening.
    pub async fn shut_down(self, drain: Duration) -> io::Result<()> {
        self.app.draining.store(true, Ordering::Relaxed);
        let metrics = &self.app.metrics;
        let deadline = Instant::now() + drain;
        while Metrics::get(&metrics.matches_active) > 0 && Instant::now() < deadline {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
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

fn random_u64() -> u64 {
    getrandom::u64().expect("the operating system provides randomness")
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
            }
            .encode()
        };
        assert_eq!(read_hello(&hello(PROTOCOL_VERSION)), Ok(vec![1, 2]));
        assert_eq!(
            read_hello(&hello(PROTOCOL_VERSION + 1)),
            Err(ErrorCode::VersionMismatch)
        );
        // A future Hello this build cannot even decode still names its version.
        assert_eq!(
            read_hello(&[0, 9, 0xFF, 0xFF]),
            Err(ErrorCode::VersionMismatch)
        );
        assert_eq!(
            read_hello(&ClientMessage::Ping { nonce: 1 }.encode()),
            Err(ErrorCode::Malformed)
        );
        assert_eq!(read_hello(&[]), Err(ErrorCode::Malformed));
        assert_eq!(read_hello(&[0, 1, 9]), Err(ErrorCode::Malformed));
    }

    #[test]
    fn rate_windows_allow_the_budget_per_second_then_refuse() {
        let start = Instant::now();
        let mut window = RateWindow::new(3, start);
        assert!((0..3).all(|_| window.allow(start)));
        assert!(!window.allow(start + Duration::from_millis(999)));
        assert!(window.allow(start + Duration::from_secs(1)));
    }

    #[test]
    fn fnv_matches_the_reference_vectors() {
        assert_eq!(fnv1a(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
