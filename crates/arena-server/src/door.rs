//! The front door (ADR 0045): a stateless HTTP service that creates matches
//! for remote players. It signs one join ticket per seat with the key it
//! shares with the match servers and returns a join link per seat; the match
//! starts on the match server when the first link is opened. No accounts:
//! whoever creates a match hands the links out.
//!
//! - `POST /api/matches` with `{"team_size": 1..=3}` returns the links.
//!   Browsers must send an allowed `Origin`; each client address may create
//!   a few matches at once and then one per interval.
//! - `GET /api/queue` is the solo queues' WebSocket ([`queue`], ADR 0046):
//!   the game queues for 1v1, solo 2v2 or solo 3v3 and receives its own
//!   ticket directly when a match forms, with no link to pass around.
//! - `GET /lobby/` is a small page that calls `/api/matches` and lists the
//!   links.
//! - `GET /healthz` and `GET /metrics` for operations.
//!
//! Tickets travel only in response bodies and in the links' fragments,
//! which browsers never send to a server. Nothing here logs them.

use std::fmt::Write;
use std::io;
use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::AtomicU64;

mod auth;
mod browser;
mod queue;
pub use auth::Me;
pub use queue::ToClient as QueueMessage;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE, RETRY_AFTER};
use axum::http::{HeaderMap, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::identity::{Identity, IdentityConfig};
use crate::ingress::{self, Cidr, OpenPerAddress, Origins, RatePerAddress};
use crate::logging::MatchId;
use crate::metrics::{Histogram, Metrics, write_metric};
use crate::ticket::{self, TicketKey};

pub struct DoorConfig {
    pub bind: SocketAddr,
    pub key: TicketKey,
    /// The `ws://` or `wss://` URL of the match server's `/play` endpoint,
    /// as players reach it.
    pub match_server: String,
    /// The game page join links open, as players reach it.
    pub page: String,
    /// Pages whose browsers may create matches.
    pub origins: Origins,
    /// Reverse proxies whose `X-Forwarded-For` names the client.
    pub trusted_proxies: Vec<Cidr>,
    pub ticket_ttl: Duration,
    /// Matches one address may create at once ...
    pub creation_burst: u32,
    /// ... and then one per this interval.
    pub creation_every: Duration,
    /// How long players have to accept a found match.
    pub ready_timeout: Duration,
    /// Open queue connections one client address may hold.
    pub queue_per_address: u32,
    /// Accounts and login (ADR 0055); `None` keeps the door guest-only.
    pub identity: Option<IdentityConfig>,
}
impl DoorConfig {
    pub fn new(key: TicketKey, match_server: String, page: String) -> Self {
        Self {
            bind: SocketAddr::from(([127, 0, 0, 1], 8090)),
            key,
            match_server,
            page,
            origins: Origins::Any,
            trusted_proxies: Vec::new(),
            ticket_ttl: Duration::from_secs(2 * 60 * 60),
            creation_burst: 5,
            creation_every: Duration::from_secs(30),
            ready_timeout: Duration::from_secs(15),
            queue_per_address: 12,
            identity: None,
        }
    }
}

/// Upper bounds of `arena_door_queue_wait_milliseconds`: 1 s to 10 min.
const QUEUE_WAIT_BUCKETS_MS: [u64; 11] = [
    1_000, 2_000, 5_000, 10_000, 20_000, 30_000, 60_000, 120_000, 180_000, 300_000, 600_000,
];

#[derive(Debug)]
struct QueueWaits(Histogram<{ QUEUE_WAIT_BUCKETS_MS.len() }>);

impl Default for QueueWaits {
    fn default() -> Self {
        Self(Histogram::new(&QUEUE_WAIT_BUCKETS_MS))
    }
}

#[derive(Debug, Default)]
struct DoorMetrics {
    issued: AtomicU64,
    /// Join tickets signed: one per seat and per spectator.
    tickets: AtomicU64,
    refused_rate: AtomicU64,
    refused_origin: AtomicU64,
    /// Players in a queue or ready check right now.
    queued: AtomicU64,
    /// The same per queue, indexed by `Mode::index`.
    queued_by_mode: [AtomicU64; 3],
    /// Open `/api/queue` connections that said hello.
    queue_connections: AtomicU64,
    /// Parties with an invite code, and private custom lobbies.
    parties: AtomicU64,
    lobbies: AtomicU64,
    queue_matches: AtomicU64,
    custom_matches: AtomicU64,
    /// Copied from the matchmaker's running totals.
    checks_started: AtomicU64,
    checks_completed: AtomicU64,
    checks_failed: AtomicU64,
    accepts: AtomicU64,
    declines: AtomicU64,
    /// Searches started, by party size 1, 2 and 3.
    searches_by_size: [AtomicU64; crate::matchmaker::MAX_PARTY],
    /// How long each party searched before its match was found.
    queue_wait: QueueWaits,
    /// Browser reports accepted, by kind (`browser::KINDS`), and refused for
    /// their shape.
    browser_reports: [AtomicU64; browser::KINDS.len()],
    browser_rejected: AtomicU64,
    /// Browser reports refused for their origin, then for the rate limit.
    browser_refused: [AtomicU64; 2],
    /// Completed logins, failed ones, and deleted accounts.
    logins: AtomicU64,
    login_failures: AtomicU64,
    deletions: AtomicU64,
}

struct Door {
    config: DoorConfig,
    identity: Option<Identity>,
    /// Login routes per address.
    auth_rate: RatePerAddress,
    creations: RatePerAddress,
    /// Browser telemetry reports per address.
    reports: RatePerAddress,
    metrics: DoorMetrics,
    queue: mpsc::Sender<queue::Command>,
    queue_open: Arc<OpenPerAddress>,
    next_conn: AtomicU64,
    shutdown: watch::Sender<bool>,
}

#[derive(Debug, Deserialize)]
struct CreateRequest {
    team_size: usize,
}

/// A created match, as the lobby shows it.
#[derive(Debug, Serialize, Deserialize)]
pub struct CreatedMatch {
    /// Hexadecimal, for reference in logs and bug reports.
    pub match_id: String,
    pub team_size: usize,
    /// Unix seconds after which the links stop working.
    pub expires: u64,
    pub seats: Vec<SeatLink>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SeatLink {
    /// Actor index: even seats are team A, odd seats team B.
    pub seat: usize,
    pub team: String,
    /// The game page with the seat's ticket in the fragment.
    pub link: String,
}

/// Percent-encodes everything but RFC 3986 unreserved characters, as
/// `application/x-www-form-urlencoded` readers (`URLSearchParams`) accept.
fn form_encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// The game page with a seat's ticket in its fragment. The browser client
/// reads `server`, `ticket` and `expires` from it and then removes it.
pub fn join_link(page: &str, server: &str, ticket: &str, expires: u64) -> String {
    let page = page.split('#').next().unwrap_or(page);
    format!(
        "{page}#server={}&ticket={ticket}&expires={expires}",
        form_encode(server)
    )
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs())
}

fn no_store(mut response: Response) -> Response {
    response
        .headers_mut()
        .insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

async fn create(
    State(door): State<Arc<Door>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    request: Result<Json<CreateRequest>, axum::extract::rejection::JsonRejection>,
) -> Response {
    if !door.config.origins.allows(&headers) {
        Metrics::add(&door.metrics.refused_origin, 1);
        return (StatusCode::FORBIDDEN, "origin not allowed").into_response();
    }
    let request = match request {
        Ok(Json(request)) => request,
        // 413 too large, 415 not JSON, 400 or 422 the wrong JSON.
        Err(rejection) => {
            return (rejection.status(), "send JSON such as {\"team_size\": 1}").into_response();
        }
    };
    if !(1..=arena_sim::map::MAX_TEAM).contains(&request.team_size) {
        return (StatusCode::BAD_REQUEST, "team_size must be 1, 2 or 3").into_response();
    }
    let address = ingress::client_address(peer, &headers, &door.config.trusted_proxies);
    if let Err(wait) = door.creations.take(address, Instant::now()) {
        Metrics::add(&door.metrics.refused_rate, 1);
        let seconds = wait.as_secs_f64().ceil().max(1.0) as u64;
        let mut response = (
            StatusCode::TOO_MANY_REQUESTS,
            format!("too many new matches from this address; try again in {seconds} s"),
        )
            .into_response();
        response
            .headers_mut()
            .insert(RETRY_AFTER, HeaderValue::from(seconds));
        return response;
    }
    let expires = unix_now() + door.config.ticket_ttl.as_secs();
    let Some(created) = ticket::issue_match(&door.config.key, request.team_size, expires) else {
        return StatusCode::INTERNAL_SERVER_ERROR.into_response();
    };
    Metrics::add(&door.metrics.issued, 1);
    Metrics::add(&door.metrics.tickets, created.tickets.len() as u64);
    tracing::info!(
        match_id = %MatchId(created.match_id),
        team_size = created.team_size,
        tickets = created.tickets.len(),
        "match links issued"
    );
    let seats = created
        .tickets
        .iter()
        .enumerate()
        .map(|(seat, ticket)| SeatLink {
            seat,
            team: if seat % 2 == 0 { "A" } else { "B" }.into(),
            link: join_link(
                &door.config.page,
                &door.config.match_server,
                ticket,
                expires,
            ),
        })
        .collect();
    no_store(
        Json(CreatedMatch {
            match_id: format!("{:016x}", created.match_id),
            team_size: created.team_size,
            expires,
            seats,
        })
        .into_response(),
    )
}

async fn render_metrics(State(door): State<Arc<Door>>) -> String {
    let mut text = render_door_metrics(&door.metrics);
    crate::metrics::ProcessStats::read().render(&mut text);
    crate::metrics::render_runtime(&mut text);
    text
}

fn render_door_metrics(metrics: &DoorMetrics) -> String {
    let mut text = String::new();
    let _ = writeln!(
        text,
        "# HELP arena_door_queued Players in a queue or ready check.\n# TYPE arena_door_queued gauge\narena_door_queued {}",
        Metrics::get(&metrics.queued)
    );
    let _ = writeln!(
        text,
        "# HELP arena_door_queued_by_mode Players in a queue or ready check, by queue.\n# TYPE arena_door_queued_by_mode gauge"
    );
    for (mode, counter) in crate::matchmaker::Mode::ALL
        .iter()
        .zip(&metrics.queued_by_mode)
    {
        let _ = writeln!(
            text,
            "arena_door_queued_by_mode{{mode=\"{}\"}} {}",
            mode.id(),
            Metrics::get(counter)
        );
    }
    for (name, help, gauge) in [
        (
            "door_queue_connections",
            "Open queue connections that said hello.",
            &metrics.queue_connections,
        ),
        (
            "door_parties",
            "Parties with an invite code.",
            &metrics.parties,
        ),
        (
            "door_lobbies",
            "Open private custom lobbies.",
            &metrics.lobbies,
        ),
    ] {
        write_metric(
            &mut text,
            &format!("arena_{name}"),
            "gauge",
            help,
            Metrics::get(gauge),
        );
    }
    for (name, help, counter) in [
        (
            "door_queue_matches_total",
            "Matches formed from the queues.",
            &metrics.queue_matches,
        ),
        (
            "door_custom_matches_total",
            "Matches started from private custom lobbies.",
            &metrics.custom_matches,
        ),
        (
            "door_matches_issued_total",
            "Matches whose tickets were issued.",
            &metrics.issued,
        ),
        (
            "door_tickets_issued_total",
            "Join tickets signed, one per seat and per spectator.",
            &metrics.tickets,
        ),
        (
            "door_logins_total",
            "Completed account logins.",
            &metrics.logins,
        ),
        (
            "door_login_failures_total",
            "Logins refused or failed (state, provider or store).",
            &metrics.login_failures,
        ),
        (
            "door_account_deletions_total",
            "Accounts deleted by their players.",
            &metrics.deletions,
        ),
        (
            "door_ready_checks_started_total",
            "Ready checks offered when a queue could fill a match.",
            &metrics.checks_started,
        ),
        (
            "door_ready_checks_completed_total",
            "Ready checks everyone accepted.",
            &metrics.checks_completed,
        ),
        (
            "door_ready_checks_failed_total",
            "Ready checks ended by a decline, a timeout or a player leaving.",
            &metrics.checks_failed,
        ),
        (
            "door_ready_accepts_total",
            "Players who accepted a ready check (a later decline is counted in declines too).",
            &metrics.accepts,
        ),
        (
            "door_ready_declines_total",
            "Players who declined a ready check.",
            &metrics.declines,
        ),
        (
            "door_refused_rate_total",
            "Match creations and queue connections refused by a per-address limit.",
            &metrics.refused_rate,
        ),
        (
            "door_refused_origin_total",
            "Match creations and queue connections refused because their page origin is not allowed.",
            &metrics.refused_origin,
        ),
    ] {
        write_metric(
            &mut text,
            &format!("arena_{name}"),
            "counter",
            help,
            Metrics::get(counter),
        );
    }
    let _ = writeln!(
        text,
        "# HELP arena_door_searches_total Queue searches started, by party size.\n# TYPE arena_door_searches_total counter"
    );
    for (size, counter) in metrics.searches_by_size.iter().enumerate() {
        let _ = writeln!(
            text,
            "arena_door_searches_total{{party_size=\"{}\"}} {}",
            size + 1,
            Metrics::get(counter)
        );
    }
    let _ = writeln!(
        text,
        "# HELP arena_door_browser_reports_total Scrubbed browser reports accepted, by kind.\n# TYPE arena_door_browser_reports_total counter"
    );
    for (kind, counter) in browser::KINDS.iter().zip(&metrics.browser_reports) {
        let _ = writeln!(
            text,
            "arena_door_browser_reports_total{{kind=\"{kind}\"}} {}",
            Metrics::get(counter)
        );
    }
    let _ = writeln!(
        text,
        "# HELP arena_door_browser_reports_refused_total Browser reports refused before reading, by reason.\n# TYPE arena_door_browser_reports_refused_total counter"
    );
    for (reason, counter) in ["origin", "rate"].iter().zip(&metrics.browser_refused) {
        let _ = writeln!(
            text,
            "arena_door_browser_reports_refused_total{{reason=\"{reason}\"}} {}",
            Metrics::get(counter)
        );
    }
    write_metric(
        &mut text,
        "arena_door_browser_reports_rejected_total",
        "counter",
        "Browser reports refused for their shape or values.",
        Metrics::get(&metrics.browser_rejected),
    );
    metrics.queue_wait.0.render(
        &mut text,
        "door_queue_wait_milliseconds",
        "How long parties searched before their match was found, in milliseconds.",
    );
    text
}

const LOBBY_HTML: &str = include_str!("door/lobby.html");
const LOBBY_JS: &str = include_str!("door/lobby.js");
const LOBBY_CSS: &str = include_str!("door/lobby.css");
/// Scripts and styles only from this service; requests only to it.
const LOBBY_POLICY: &str = "default-src 'none'; script-src 'self'; style-src 'self'; \
     connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'";

/// The closed palette (docs/palette.json) as CSS custom properties.
fn palette_css() -> String {
    let mut css = String::from(":root {\n");
    for swatch in arena_palette::ALL {
        let _ = writeln!(css, "  --pal-{}: {};", swatch.id, swatch.hex());
    }
    css.push_str("}\n");
    css
}

fn page(content_type: &'static str, body: impl Into<axum::body::Body>) -> Response {
    let mut response = Response::new(body.into());
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(content_type));
    headers.insert(
        HeaderName::from_static("content-security-policy"),
        HeaderValue::from_static(LOBBY_POLICY),
    );
    headers.insert(
        HeaderName::from_static("referrer-policy"),
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(
        HeaderName::from_static("x-content-type-options"),
        HeaderValue::from_static("nosniff"),
    );
    response
}

/// The door's routes, with its matchmaker task started. Must be called
/// inside the Tokio runtime.
fn build(config: DoorConfig) -> io::Result<(Router, Arc<Door>)> {
    let identity = config
        .identity
        .clone()
        .map(|identity| Identity::start(identity, &config.page))
        .transpose()
        .map_err(io::Error::other)?;
    let (commands, receiver) = mpsc::channel(1024);
    let door = Arc::new(Door {
        identity,
        auth_rate: RatePerAddress::new(20, Duration::from_secs(6)),
        creations: RatePerAddress::new(config.creation_burst, config.creation_every),
        // A visit sends a handful (startup, a summary per match, a few errors).
        reports: RatePerAddress::new(30, Duration::from_secs(10)),
        queue_open: OpenPerAddress::new(config.queue_per_address),
        config,
        metrics: DoorMetrics::default(),
        queue: commands,
        next_conn: AtomicU64::new(1),
        shutdown: watch::Sender::new(false),
    });
    tokio::spawn(queue::run(door.clone(), receiver));
    let palette = palette_css();
    let router = Router::new()
        .route(
            "/api/matches",
            post(create).layer(DefaultBodyLimit::max(1024)),
        )
        .route("/api/queue", get(queue::upgrade))
        .route(
            "/api/telemetry",
            post(browser::receive).layer(DefaultBodyLimit::max(browser::MAX_REPORT)),
        )
        .route("/lobby", get(|| async { Redirect::permanent("/lobby/") }))
        .route(
            "/lobby/",
            get(|| async { page("text/html; charset=utf-8", LOBBY_HTML) }),
        )
        .route(
            "/lobby/lobby.js",
            get(|| async { page("text/javascript; charset=utf-8", LOBBY_JS) }),
        )
        .route(
            "/lobby/lobby.css",
            get(|| async { page("text/css; charset=utf-8", LOBBY_CSS) }),
        )
        .route(
            "/lobby/palette.css",
            get(move || {
                let palette = palette.clone();
                async move { page("text/css; charset=utf-8", palette) }
            }),
        )
        .route("/api/auth/me", get(auth::me))
        .route("/api/auth/{provider}/login", get(auth::login))
        .route("/api/auth/{provider}/callback", get(auth::callback))
        .route("/api/auth/logout", post(auth::logout))
        .route("/api/auth/logout_everywhere", post(auth::logout_everywhere))
        .route("/api/auth/delete", post(auth::delete))
        .route("/healthz", get(|| async { "ok" }))
        .route("/metrics", get(render_metrics))
        .with_state(door.clone());
    let router = crate::traced(router);
    Ok((router, door))
}

/// A started front door.
pub struct DoorRunning {
    pub addr: SocketAddr,
    stop: watch::Sender<bool>,
    task: JoinHandle<io::Result<()>>,
}
impl DoorRunning {
    pub async fn stop(self) -> io::Result<()> {
        self.stop.send_replace(true);
        self.task.await.map_err(io::Error::other)?
    }
}

pub async fn start(config: DoorConfig) -> io::Result<DoorRunning> {
    let listener = tokio::net::TcpListener::bind(config.bind).await?;
    let addr = listener.local_addr()?;
    let (router, door) = build(config)?;
    let stop = door.shutdown.clone();
    let mut stopped = stop.subscribe();
    let service = router.into_make_service_with_connect_info::<SocketAddr>();
    let server = axum::serve(listener, service).with_graceful_shutdown(async move {
        let _ = stopped.wait_for(|stop| *stop).await;
    });
    let task = tokio::spawn(async move { server.await });
    Ok(DoorRunning { addr, stop, task })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_links_carry_the_seat_in_the_fragment_only() {
        let link = join_link(
            "https://arena.example.com/#old",
            "wss://arena.example.com/play",
            "00ff",
            123,
        );
        assert_eq!(
            link,
            "https://arena.example.com/#server=wss%3A%2F%2Farena.example.com%2Fplay&ticket=00ff&expires=123"
        );
        assert_eq!(form_encode("a b&c=d"), "a%20b%26c%3Dd");
    }

    #[test]
    fn the_palette_stylesheet_lists_every_swatch() {
        let css = palette_css();
        assert_eq!(css.matches("--pal-").count(), arena_palette::ALL.len());
        assert!(css.contains("--pal-night-1: #0a1118;"), "{css}");
    }

    #[test]
    fn the_lobby_uses_only_palette_colors() {
        // Colors come from var(--pal-*) only (docs/palette.json rules).
        for (name, text) in [("lobby.css", LOBBY_CSS), ("lobby.html", LOBBY_HTML)] {
            for banned in ["#", "rgb(", "hsl("] {
                let found = text
                    .lines()
                    .filter(|line| !line.trim_start().starts_with("<!--"))
                    .filter(|line| line.contains(banned))
                    .filter(|line| !(banned == "#" && line.contains("href=\"#")))
                    .count();
                assert_eq!(found, 0, "{name} uses a literal color ({banned})");
            }
        }
    }
}
