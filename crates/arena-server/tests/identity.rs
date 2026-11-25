//! Accounts end to end (ADR 0055) against a fake WorkOS on loopback and a
//! real PostgreSQL (`ARENA_TEST_DATABASE_URL`; each door gets a fresh
//! schema): login with state and PKCE, the session cookie, the account id
//! in queue tickets, and every way a cookie must stop working.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arena_server::door::{self, DoorConfig, Me, QueueMessage};
use arena_server::identity::IdentityConfig;
use arena_server::identity::cookie::{SessionKey, base64url, pkce_challenge};
use arena_server::identity::names::Blocklist;
use arena_server::identity::store::test_database;
use arena_server::identity::workos::WorkOs;
use arena_server::ingress::Origins;
use arena_server::ticket::{Claims, TicketKey, from_hex};
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{delete, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const PAGE: &str = "https://arena.example.com/";
const ORIGIN: &str = "Origin: https://arena.example.com\r\n";
/// The WorkOS user behind the default fake login (`good-code`).
const SUBJECT: &str = "user_01HDEFAULT";
const WAIT: Duration = Duration::from_secs(5);

/// The fake WorkOS: the PKCE challenge it expects, the users deleted there,
/// and whether deleting fails.
#[derive(Clone, Default)]
struct Fake(
    Arc<Mutex<Option<String>>>,
    Arc<Mutex<Vec<String>>>,
    Arc<std::sync::atomic::AtomicBool>,
);

/// A WorkOS-shaped access token whose `sid` is `session_<who>`.
fn access_token(who: &str) -> String {
    let claims = serde_json::json!({"sid": format!("session_{who}"), "sub": "user"});
    format!(
        "{}.{}.signature",
        base64url(br#"{"alg":"RS256"}"#),
        base64url(claims.to_string().as_bytes())
    )
}

async fn authenticate(
    State(fake): State<Fake>,
    Json(body): Json<HashMap<String, serde_json::Value>>,
) -> (StatusCode, Json<serde_json::Value>) {
    let field = |name: &str| body.get(name).and_then(|v| v.as_str()).unwrap_or_default();
    let expected = fake.0.lock().unwrap().clone();
    let ok = field("code").starts_with("good-")
        && field("grant_type") == "authorization_code"
        && field("client_id") == "client_test"
        && field("client_secret") == "sk_test"
        && expected.is_some()
        && Some(pkce_challenge(field("code_verifier"))) == expected;
    if !ok {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "invalid_grant"})),
        );
    }
    // "good-code" is the default player; "good-<who>" another one.
    let who = field("code").trim_start_matches("good-").to_owned();
    let user = if who == "code" {
        SUBJECT.to_owned()
    } else {
        format!("user_{who}")
    };
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "user": {"object": "user", "id": user, "email": format!("{who}@example.com")},
            "access_token": access_token(&who),
            "refresh_token": "refresh",
        })),
    )
}

async fn delete_user(
    State(fake): State<Fake>,
    headers: HeaderMap,
    Path(user): Path<String>,
) -> StatusCode {
    if headers.get("authorization").and_then(|v| v.to_str().ok()) != Some("Bearer sk_test") {
        return StatusCode::UNAUTHORIZED;
    }
    if fake.2.load(std::sync::atomic::Ordering::Relaxed) {
        return StatusCode::SERVICE_UNAVAILABLE;
    }
    fake.1.lock().unwrap().push(user);
    StatusCode::OK
}

async fn fake_provider(fake: Fake) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/user_management/authenticate", post(authenticate))
        .route("/user_management/users/{user}", delete(delete_user))
        .with_state(fake);
    tokio::spawn(async move { axum::serve(listener, app).await });
    addr
}

/// Whether these tests must skip: they need `ARENA_TEST_DATABASE_URL`.
fn no_database() -> bool {
    let missing = std::env::var_os("ARENA_TEST_DATABASE_URL").is_none();
    if missing {
        eprintln!("skipped: set ARENA_TEST_DATABASE_URL to run account tests");
    }
    missing
}

struct Reply {
    status: u16,
    /// Header lines as received.
    headers: Vec<(String, String)>,
    body: String,
}
impl Reply {
    fn header(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }
    /// The value a `Set-Cookie` gives `name`, and its `Max-Age`.
    fn cookie(&self, name: &str) -> Option<(String, String)> {
        self.header("set-cookie").into_iter().find_map(|line| {
            let (pair, attributes) = line.split_once(';')?;
            let value = pair.strip_prefix(&format!("{name}="))?;
            let age = attributes
                .split(';')
                .find_map(|a| a.trim().strip_prefix("Max-Age="))?;
            Some((value.to_owned(), age.to_owned()))
        })
    }
}

async fn request(addr: SocketAddr, method: &str, path: &str, extra: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\n{extra}Content-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    let mut lines = head.lines();
    let status = lines.next().unwrap()[9..12].parse().unwrap();
    let headers = lines
        .filter_map(|line| line.split_once(": "))
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    Reply {
        status,
        headers,
        body: body.to_owned(),
    }
}

async fn door_with(origins: Origins, provider: SocketAddr) -> (door::DoorRunning, TicketKey) {
    door_tuned(origins, provider, |_| {}).await
}

/// A door with `tune` applied to its configuration.
async fn door_tuned(
    origins: Origins,
    provider: SocketAddr,
    tune: impl FnOnce(&mut DoorConfig),
) -> (door::DoorRunning, TicketKey) {
    let key = TicketKey::random();
    let database = test_database().await.expect("ARENA_TEST_DATABASE_URL set");
    let mut config = DoorConfig::new(key.clone(), "ws://127.0.0.1:1/play".into(), PAGE.into());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.origins = origins;
    config.identity = Some(IdentityConfig {
        database,
        key: SessionKey::random(),
        workos: WorkOs {
            client_id: "client_test".into(),
            api_key: "sk_test".into(),
            api: format!("http://{provider}"),
        },
        admins: HashSet::new(),
        blocked: Blocklist::new(""),
    });
    tune(&mut config);
    (door::start(config).await.unwrap(), key)
}

/// Starts a login; returns the login cookie and the WorkOS link.
async fn start_login(addr: SocketAddr) -> (String, reqwest::Url) {
    let started = request(addr, "GET", "/api/auth/login", "").await;
    assert_eq!(started.status, 303);
    let (oauth, age) = started.cookie("__Host-arena_oauth").unwrap();
    assert_eq!(age, "600");
    let link = reqwest::Url::parse(started.header("location")[0]).unwrap();
    (oauth, link)
}

fn query(link: &reqwest::Url) -> HashMap<String, String> {
    link.query_pairs().into_owned().collect()
}

/// Logs in through the door and the fake provider; returns the session
/// cookie value.
async fn log_in(addr: SocketAddr, fake: &Fake) -> String {
    log_in_as(addr, fake, "code").await
}

/// Logs in as another player of the fake provider.
async fn log_in_as(addr: SocketAddr, fake: &Fake, who: &str) -> String {
    let (oauth, link) = start_login(addr).await;
    let query = query(&link);
    assert_eq!(query["provider"], "authkit");
    assert_eq!(query["client_id"], "client_test");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(
        query["redirect_uri"],
        "https://arena.example.com/api/auth/callback"
    );
    *fake.0.lock().unwrap() = Some(query["code_challenge"].clone());
    let back = request(
        addr,
        "GET",
        &format!(
            "/api/auth/callback?code=good-{who}&state={}",
            query["state"]
        ),
        &format!("Cookie: __Host-arena_oauth={oauth}\r\n"),
    )
    .await;
    assert_eq!(back.status, 303, "{}", back.body);
    assert_eq!(
        back.header("location"),
        [PAGE],
        "always back to the game page"
    );
    assert_eq!(
        back.cookie("__Host-arena_oauth").unwrap().1,
        "0",
        "login cookie cleared"
    );
    let set = back.header("set-cookie").join("\n");
    assert!(set.contains("HttpOnly; Secure; SameSite=Lax"), "{set}");
    back.cookie("__Host-arena_session").unwrap().0
}

/// `POST /api/auth/handle` with `body` (and our Origin when `origin`); the
/// status and JSON answer.
async fn post_handle(
    addr: SocketAddr,
    cookie: &str,
    body: &str,
    origin: bool,
) -> (u16, serde_json::Value) {
    let origin = if origin { ORIGIN } else { "" };
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /api/auth/handle HTTP/1.1\r\nHost: test\r\nCookie: __Host-arena_session={cookie}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let json = response.split_once("\r\n\r\n").unwrap().1;
    (
        status,
        serde_json::from_str(json).unwrap_or(serde_json::Value::Null),
    )
}

/// Picks `name` with our Origin.
async fn choose(addr: SocketAddr, cookie: &str, name: &str) -> (u16, serde_json::Value) {
    post_handle(addr, cookie, &format!(r#"{{"name":"{name}"}}"#), true).await
}

async fn me(addr: SocketAddr, cookie: &str) -> Me {
    let reply = request(
        addr,
        "GET",
        "/api/auth/me",
        &format!("Cookie: __Host-arena_session={cookie}\r\n"),
    )
    .await;
    assert_eq!(reply.status, 200);
    serde_json::from_str(&reply.body).unwrap()
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// Opens a queue connection (with `cookie`, and our `Origin` when `origin`)
/// and queues for a duel, returning once the search is under way.
async fn open_queue(door: SocketAddr, guest: u128, cookie: Option<&str>, origin: bool) -> Socket {
    let mut request = format!("ws://{door}/api/queue")
        .into_client_request()
        .unwrap();
    if origin {
        request
            .headers_mut()
            .insert("origin", "https://arena.example.com".parse().unwrap());
    }
    if let Some(cookie) = cookie {
        request.headers_mut().insert(
            "cookie",
            format!("__Host-arena_session={cookie}").parse().unwrap(),
        );
    }
    let (mut socket, _) = connect_async(request).await.unwrap();
    for json in [
        format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#),
        r#"{"type":"queue","mode":"1v1"}"#.to_owned(),
    ] {
        socket.send(Message::text(json)).await.unwrap();
    }
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        match serde_json::from_str::<QueueMessage>(&text).unwrap() {
            QueueMessage::Queued { .. } => return socket,
            QueueMessage::ReadyCheck { you: false, .. } => {
                socket
                    .send(Message::text(r#"{"type":"accept"}"#))
                    .await
                    .unwrap();
                return socket;
            }
            _ => {}
        }
    }
}

/// Accepts the ready check when offered and returns this player's ticket.
async fn await_ticket(mut socket: Socket) -> String {
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        match serde_json::from_str::<QueueMessage>(&text).unwrap() {
            QueueMessage::ReadyCheck { you: false, .. } => {
                socket
                    .send(Message::text(r#"{"type":"accept"}"#))
                    .await
                    .unwrap();
            }
            QueueMessage::Match { ticket, .. } => return ticket,
            _ => {}
        }
    }
}

/// Queues for a duel (with `cookie`, if any), accepts, returns the ticket.
async fn queue_ticket(door: SocketAddr, guest: u128, cookie: Option<&str>) -> String {
    await_ticket(open_queue(door, guest, cookie, true).await).await
}

#[tokio::test]
async fn players_log_in_and_their_account_rides_in_their_ticket() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    // Room for this test's own logins; the limit is checked at the end.
    let (door, key) = door_tuned(
        Origins::Only(vec!["https://arena.example.com".into()]),
        provider,
        |config| config.auth_burst = 30,
    )
    .await;
    let addr = door.addr;
    let claims = |ticket: &str| -> Claims { key.verify(&from_hex(ticket).unwrap(), 0).unwrap() };

    // A guest: no account, and login on offer.
    let guest: Me =
        serde_json::from_str(&request(addr, "GET", "/api/auth/me", "").await.body).unwrap();
    assert_eq!(
        guest,
        Me {
            account: None,
            admin: false,
            login: true,
            handle: None,
            rename_at: None,
        }
    );
    // Asking to sign up opens WorkOS's sign-up screen.
    let sign_up = request(addr, "GET", "/api/auth/login?sign_up", "").await;
    assert!(
        sign_up.header("location")[0].contains("screen_hint=sign-up"),
        "{:?}",
        sign_up.header("location")
    );

    let session = log_in(addr, &fake).await;
    let account = me(addr, &session).await.account.expect("logged in");
    // Logging in again finds the same account.
    assert_eq!(
        me(addr, &log_in(addr, &fake).await).await.account,
        Some(account.clone())
    );

    // The logged-in player's ticket carries the account; the guest's not.
    let (logged_in, guest_ticket) = tokio::join!(
        queue_ticket(addr, 1, Some(&session)),
        queue_ticket(addr, 2, None)
    );
    assert_eq!(
        claims(&logged_in).account,
        Some(u64::from_str_radix(&account, 16).unwrap())
    );
    assert_eq!(claims(&guest_ticket).account, None);
    // Without our Origin, the queue handshake carries no account.
    let (no_origin, guest_two) = tokio::join!(
        async { await_ticket(open_queue(addr, 3, Some(&session), false).await).await },
        queue_ticket(addr, 4, None)
    );
    assert_eq!(claims(&no_origin).account, None);
    assert_eq!(claims(&guest_two).account, None);

    // Forged state never logs in, even when the provider would accept the
    // code and verifier: only the state check can stop it.
    let (oauth, link) = start_login(addr).await;
    *fake.0.lock().unwrap() = Some(query(&link)["code_challenge"].clone());
    let forged = request(
        addr,
        "GET",
        "/api/auth/callback?code=good-code&state=someone-elses",
        &format!("Cookie: __Host-arena_oauth={oauth}\r\n"),
    )
    .await;
    assert_eq!(forged.status, 400);
    assert!(forged.cookie("__Host-arena_session").is_none());
    // Nor does a callback without a login started, nor one WorkOS reports
    // as refused or cancelled.
    let unstarted = request(addr, "GET", "/api/auth/callback?code=good-code&state=x", "").await;
    assert_eq!(unstarted.status, 400);
    let refused = request(
        addr,
        "GET",
        &format!(
            "/api/auth/callback?error=access_denied&code=good-code&state={}",
            query(&link)["state"]
        ),
        &format!("Cookie: __Host-arena_oauth={oauth}\r\n"),
    )
    .await;
    assert_eq!(refused.status, 400);
    assert!(refused.cookie("__Host-arena_session").is_none());
    // A tampered session is a guest.
    // Always a different last digit (replacing a `0` with `0` would not
    // tamper at all).
    let last = if session.ends_with('0') { '1' } else { '0' };
    let tampered = format!("{}{last}", &session[..session.len() - 1]);
    assert_eq!(me(addr, &tampered).await.account, None);

    // Acting needs our Origin: without it, nothing happens.
    let cookie = format!("Cookie: __Host-arena_session={session}\r\n");
    assert_eq!(
        request(addr, "POST", "/api/auth/logout_everywhere", &cookie)
            .await
            .status,
        403
    );
    assert_eq!(
        request(
            addr,
            "POST",
            "/api/auth/logout_everywhere",
            &format!("{cookie}Origin: https://evil.example\r\n")
        )
        .await
        .status,
        403
    );
    assert!(me(addr, &session).await.account.is_some());
    // Logging out everywhere ends this session and any copy of it, and
    // hands the page WorkOS's link to end its own session too.
    let other_device = log_in(addr, &fake).await;
    let ended = request(
        addr,
        "POST",
        "/api/auth/logout_everywhere",
        &format!("{cookie}{ORIGIN}"),
    )
    .await;
    assert_eq!(ended.status, 200);
    assert_eq!(ended.cookie("__Host-arena_session").unwrap().1, "0");
    let logout: serde_json::Value = serde_json::from_str(&ended.body).unwrap();
    let logout = reqwest::Url::parse(logout["logout_url"].as_str().unwrap()).unwrap();
    assert_eq!(logout.path(), "/user_management/sessions/logout");
    let logout = query(&logout);
    assert_eq!(logout["session_id"], "session_code");
    assert_eq!(logout["return_to"], PAGE);
    assert_eq!(me(addr, &session).await.account, None);
    assert_eq!(me(addr, &other_device).await.account, None);

    // Deleting the account ends it for good; the next login is a new one.
    let fresh = log_in(addr, &fake).await;
    assert_eq!(me(addr, &fresh).await.account, Some(account.clone()));
    let fresh_cookie = format!("Cookie: __Host-arena_session={fresh}\r\n");
    assert_eq!(
        request(addr, "POST", "/api/auth/delete", &fresh_cookie)
            .await
            .status,
        403,
        "deleting needs our Origin"
    );
    // If WorkOS cannot delete the user, nothing is deleted.
    fake.2.store(true, std::sync::atomic::Ordering::Relaxed);
    let failed = request(
        addr,
        "POST",
        "/api/auth/delete",
        &format!("{fresh_cookie}{ORIGIN}"),
    )
    .await;
    assert_eq!(failed.status, 502);
    assert_eq!(me(addr, &fresh).await.account, Some(account.clone()));
    fake.2.store(false, std::sync::atomic::Ordering::Relaxed);
    // A search under way when the account is deleted gets no account.
    let searching = open_queue(addr, 5, Some(&fresh), true).await;
    let deleted = request(
        addr,
        "POST",
        "/api/auth/delete",
        &format!("{fresh_cookie}{ORIGIN}"),
    )
    .await;
    assert_eq!(deleted.status, 204);
    assert_eq!(me(addr, &fresh).await.account, None);
    assert_eq!(*fake.1.lock().unwrap(), [SUBJECT], "deleted at WorkOS too");
    let (revoked, _) = tokio::join!(await_ticket(searching), queue_ticket(addr, 6, None));
    assert_eq!(
        claims(&revoked).account,
        None,
        "a deleted account rides in no ticket"
    );
    let reborn = me(addr, &log_in(addr, &fake).await).await.account.unwrap();
    assert_ne!(reborn, account);

    let metrics = request(addr, "GET", "/metrics", "").await.body;
    for line in [
        "arena_door_logins_total 5\n",
        "arena_door_login_failures_total 3\n",
        "arena_door_account_deletions_total 1\n",
    ] {
        assert!(metrics.contains(line), "missing {line:?} in {metrics}");
    }
    // Login routes are limited per address.
    let mut statuses = Vec::new();
    for _ in 0..40 {
        statuses.push(request(addr, "GET", "/api/auth/login", "").await.status);
    }
    assert!(statuses.contains(&429), "{statuses:?}");
    door.stop().await.unwrap();
}

#[tokio::test]
async fn players_pick_handles_that_ride_in_their_tickets() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let (door, key) = door_with(
        Origins::Only(vec!["https://arena.example.com".into()]),
        provider,
    )
    .await;
    let addr = door.addr;
    let claims = |ticket: &str| -> Claims { key.verify(&from_hex(ticket).unwrap(), 0).unwrap() };
    let session = log_in(addr, &fake).await;

    // Before picking, a logged-in player plays under a guest handle, as
    // guests do.
    let (before, guest) = tokio::join!(
        queue_ticket(addr, 1, Some(&session)),
        queue_ticket(addr, 2, None)
    );
    for ticket in [&before, &guest] {
        let handle = claims(ticket)
            .handle
            .expect("everyone from the queue has a handle");
        assert_eq!(handle.name(), "Guest");
    }

    // Names follow the rules; the door picks the number.
    assert_eq!(
        choose(addr, &session, "ab").await,
        (422, serde_json::json!({"error": "invalid_name"}))
    );
    assert_eq!(
        choose(addr, &session, "Adm1n").await,
        (422, serde_json::json!({"error": "blocked_name"}))
    );
    let (status, picked) = choose(addr, &session, "Rook").await;
    assert_eq!(status, 200, "{picked}");
    let handle = picked["handle"].as_str().unwrap().to_owned();
    assert!(handle.starts_with("Rook#"), "{handle}");
    let mine = me(addr, &session).await;
    assert_eq!(mine.handle.as_deref(), Some(handle.as_str()));
    assert!(mine.rename_at.is_some(), "a new name waits a week");
    // A different name within the week is refused; a case change is fine.
    let (status, refused) = choose(addr, &session, "Bishop").await;
    assert_eq!((status, refused["error"].as_str()), (429, Some("too_soon")));
    assert_eq!(refused["rename_at"].as_u64(), mine.rename_at);
    assert_eq!(choose(addr, &session, "ROOK").await.0, 200);

    // The handle rides in the player's next ticket, and a search already
    // under way picks up a new one at once.
    let searching = open_queue(addr, 3, Some(&session), true).await;
    let (status, recased) = choose(addr, &session, "rook").await;
    assert_eq!(status, 200);
    let (ticket, _) = tokio::join!(await_ticket(searching), queue_ticket(addr, 4, None));
    assert_eq!(
        claims(&ticket).handle.map(|h| h.to_string()).as_deref(),
        recased["handle"].as_str()
    );
    // Guests cannot pick, nor can requests without our Origin, nor
    // malformed bodies.
    let (status, _) = choose(addr, "nobody", "Knight").await;
    assert_eq!(status, 401);
    assert_eq!(
        post_handle(addr, &session, r#"{"name":"Knight"}"#, false)
            .await
            .0,
        403
    );
    for body in [
        r#"{"name":1}"#,
        r#"{"name":"Knight","admin":true}"#,
        "not json",
    ] {
        assert_eq!(
            post_handle(addr, &session, body, true).await,
            (422, serde_json::json!({"error": "invalid_name"})),
            "{body}"
        );
    }
    door.stop().await.unwrap();
}

#[tokio::test]
async fn cookies_never_act_for_a_door_open_to_any_origin() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let (door, key) = door_with(Origins::Any, provider).await;
    let session = log_in(door.addr, &fake).await;
    assert_eq!(me(door.addr, &session).await.account, None);
    let (ticket, _) = tokio::join!(
        queue_ticket(door.addr, 1, Some(&session)),
        queue_ticket(door.addr, 2, None)
    );
    assert_eq!(
        key.verify(&from_hex(&ticket).unwrap(), 0).unwrap().account,
        None
    );
    door.stop().await.unwrap();
}

/// `POST /api/friends/<action>` with our Origin.
async fn friend(
    addr: SocketAddr,
    cookie: &str,
    action: &str,
    handle: &str,
) -> (u16, serde_json::Value) {
    let body = format!(r#"{{"handle":"{handle}"}}"#);
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /api/friends/{action} HTTP/1.1\r\nHost: test\r\nCookie: __Host-arena_session={cookie}\r\n{ORIGIN}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let json = response.split_once("\r\n\r\n").unwrap().1;
    (
        status,
        serde_json::from_str(json).unwrap_or(serde_json::Value::Null),
    )
}

async fn friends_of(addr: SocketAddr, cookie: &str) -> serde_json::Value {
    let reply = request(
        addr,
        "GET",
        "/api/friends",
        &format!("Cookie: __Host-arena_session={cookie}\r\n"),
    )
    .await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    serde_json::from_str(&reply.body).unwrap()
}

/// A queue connection that says hello and nothing else.
async fn connect_queue(door: SocketAddr, guest: u128, cookie: &str) -> Socket {
    let mut request = format!("ws://{door}/api/queue")
        .into_client_request()
        .unwrap();
    request
        .headers_mut()
        .insert("origin", "https://arena.example.com".parse().unwrap());
    request.headers_mut().insert(
        "cookie",
        format!("__Host-arena_session={cookie}").parse().unwrap(),
    );
    let (mut socket, _) = connect_async(request).await.unwrap();
    socket
        .send(Message::text(format!(
            r#"{{"type":"hello","guest":"{guest:032x}"}}"#
        )))
        .await
        .unwrap();
    socket
}

/// Reads until a message `wanted` accepts.
async fn until<T>(socket: &mut Socket, wanted: impl Fn(QueueMessage) -> Option<T>) -> T {
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        let Message::Text(text) = frame else { continue };
        if let Some(found) = wanted(serde_json::from_str(&text).unwrap()) {
            return found;
        }
    }
}

/// Whether a message `wanted` accepts arrives within `ms`.
async fn hears(socket: &mut Socket, ms: u64, wanted: impl Fn(&QueueMessage) -> bool) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Some(Ok(frame))) = tokio::time::timeout_at(deadline, socket.next()).await {
        if let Message::Text(text) = frame
            && wanted(&serde_json::from_str(&text).unwrap())
        {
            return true;
        }
    }
    false
}

async fn say(socket: &mut Socket, json: String) {
    socket.send(Message::text(json)).await.unwrap();
}

#[tokio::test]
async fn friends_ask_accept_see_each_other_and_invite_with_one_click() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    // Many account calls from one test address: lift the per-address limit
    // (the per-account limits under test stay as deployed).
    let (door, _) = door_tuned(
        Origins::Only(vec!["https://arena.example.com".into()]),
        provider,
        |config| config.auth_burst = 1000,
    )
    .await;
    let addr = door.addr;
    let mut players = Vec::new();
    for who in ["alice", "bob", "carol", "dave"] {
        let session = log_in_as(addr, &fake, who).await;
        let (status, picked) = choose(addr, &session, &format!("{}x", who)).await;
        assert_eq!(status, 200, "{picked}");
        players.push((session, picked["handle"].as_str().unwrap().to_owned()));
    }
    let [
        (alice, alice_h),
        (bob, bob_h),
        (carol, carol_h),
        (dave, dave_h),
    ] = <[(String, String); 4]>::try_from(players).unwrap();

    // Bob is online and hears about Alice's request at once.
    let mut bob_socket = connect_queue(addr, 2, &bob).await;
    assert_eq!(
        friend(addr, &alice, "request", &bob_h).await,
        (200, serde_json::json!({"relation": "outgoing"}))
    );
    until(&mut bob_socket, |m| {
        matches!(m, QueueMessage::FriendsChanged {}).then_some(())
    })
    .await;
    // Requests show no presence: only friends learn where you are.
    assert_eq!(
        friends_of(addr, &bob).await["friends"][0],
        serde_json::json!({"handle": alice_h, "relation": "incoming"})
    );
    assert_eq!(friend(addr, &bob, "accept", &alice_h).await.0, 204);
    let listed = friends_of(addr, &alice).await;
    assert_eq!(
        listed["friends"][0],
        serde_json::json!({"handle": bob_h, "relation": "friend", "presence": "online"})
    );
    // Carol becomes a friend too, but stays offline.
    friend(addr, &carol, "request", &alice_h).await;
    assert_eq!(
        friend(addr, &alice, "request", &carol_h).await.1["relation"],
        "friend"
    );
    let carol_presence = friends_of(addr, &alice).await["friends"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["handle"] == carol_h.as_str())
        .unwrap()["presence"]
        .clone();
    assert_eq!(carol_presence, "offline");

    // A friend searching a queue shows as searching.
    say(
        &mut bob_socket,
        r#"{"type":"queue","mode":"3v3"}"#.to_owned(),
    )
    .await;
    until(&mut bob_socket, |m| {
        matches!(m, QueueMessage::Queued { .. }).then_some(())
    })
    .await;
    let presence_of_bob = |listed: serde_json::Value| {
        listed["friends"]
            .as_array()
            .unwrap()
            .iter()
            .find(|f| f["handle"] == bob_h.as_str())
            .unwrap()["presence"]
            .clone()
    };
    assert_eq!(presence_of_bob(friends_of(addr, &alice).await), "searching");
    say(&mut bob_socket, r#"{"type":"cancel"}"#.to_owned()).await;
    until(&mut bob_socket, |m| {
        matches!(m, QueueMessage::Stopped { .. }).then_some(())
    })
    .await;
    assert_eq!(presence_of_bob(friends_of(addr, &alice).await), "online");

    // One-click invite: Alice (on her own) invites Bob; she gets a party,
    // Bob gets its code and joins with it. Dave, online but no friend,
    // hears nothing.
    let mut dave_socket = connect_queue(addr, 4, &dave).await;
    let mut alice_socket = connect_queue(addr, 1, &alice).await;
    say(
        &mut alice_socket,
        format!(r#"{{"type":"invite","handle":"{bob_h}"}}"#),
    )
    .await;
    let (from, kind, code) = until(&mut bob_socket, |m| match m {
        QueueMessage::Invite { from, kind, code } => Some((from, kind, code)),
        _ => None,
    })
    .await;
    assert_eq!((from.as_str(), kind.as_str()), (alice_h.as_str(), "party"));
    until(&mut alice_socket, |m| {
        matches!(m, QueueMessage::InviteSent { ref to } if *to == bob_h).then_some(())
    })
    .await;
    say(
        &mut bob_socket,
        format!(r#"{{"type":"join_party","code":"{code}"}}"#),
    )
    .await;
    let members = until(&mut alice_socket, |m| match m {
        QueueMessage::Party { members, .. } if members == 2 => Some(members),
        _ => None,
    })
    .await;
    assert_eq!(members, 2);

    // Offline friends, strangers and yourself cannot be invited.
    for (handle, code) in [
        (&carol_h, "offline"),
        (&dave_h, "no_such_friend"),
        (&alice_h, "no_such_friend"),
    ] {
        say(
            &mut alice_socket,
            format!(r#"{{"type":"invite","handle":"{handle}"}}"#),
        )
        .await;
        let refused = until(&mut alice_socket, |m| match m {
            QueueMessage::Refused { code } => Some(code),
            _ => None,
        })
        .await;
        assert_eq!(refused, code, "{handle}");
    }
    // Asking yourself or nobody, and asking without a name, are refused.
    assert_eq!(friend(addr, &alice, "request", &alice_h).await.0, 422);
    assert_eq!(
        friend(addr, &alice, "request", "Nobody#0001").await,
        (404, serde_json::json!({"error": "no_such_player"}))
    );
    let nameless = log_in_as(addr, &fake, "erin").await;
    assert_eq!(
        friend(addr, &nameless, "request", &alice_h).await,
        (409, serde_json::json!({"error": "pick_a_name_first"}))
    );

    assert!(
        !hears(&mut dave_socket, 200, |m| matches!(
            m,
            QueueMessage::Invite { .. }
        ))
        .await,
        "a stranger never receives the code"
    );

    // Blocking: Dave blocks Alice, who cannot tell him from nobody, and
    // hears nothing about it.
    assert_eq!(friend(addr, &dave, "block", &alice_h).await.0, 204);
    assert!(
        !hears(&mut alice_socket, 200, |m| matches!(
            m,
            QueueMessage::FriendsChanged {}
        ))
        .await,
        "a block tells the blocked player nothing"
    );
    for action in ["unblock", "accept", "decline", "remove"] {
        assert_eq!(
            friend(addr, &alice, action, &dave_h).await,
            friend(addr, &alice, action, "Nobody#0001").await,
            "{action} answers alike for a blocker and for nobody"
        );
    }
    assert_eq!(
        friend(addr, &alice, "block", "Nobody#0001").await.0,
        friend(addr, &carol, "block", &dave_h).await.0,
        "blocking nobody looks like blocking someone"
    );
    friend(addr, &carol, "unblock", &dave_h).await;
    assert_eq!(
        friend(addr, &alice, "request", &dave_h).await,
        (404, serde_json::json!({"error": "no_such_player"}))
    );
    assert_eq!(
        friends_of(addr, &dave).await["blocked"],
        serde_json::json!([alice_h])
    );
    // Blocking a friend ends the friendship and their invites.
    assert_eq!(friend(addr, &bob, "block", &alice_h).await.0, 204);
    say(
        &mut alice_socket,
        format!(r#"{{"type":"invite","handle":"{bob_h}"}}"#),
    )
    .await;
    let refused = until(&mut alice_socket, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(refused, "no_such_friend");

    // Spam: requests and invites are limited per account.
    let mut statuses = Vec::new();
    for n in 0..12 {
        statuses.push(
            friend(addr, &alice, "request", &format!("Ghost{n}#0001"))
                .await
                .0,
        );
    }
    assert!(statuses.contains(&429), "{statuses:?}");
    // Alice already sent five invites (the burst); stay under the queue's
    // per-connection message rate while asking for more.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    let mut refusals = Vec::new();
    for _ in 0..4 {
        say(
            &mut alice_socket,
            format!(r#"{{"type":"invite","handle":"{carol_h}"}}"#),
        )
        .await;
        let refused = until(&mut alice_socket, |m| match m {
            QueueMessage::Refused { code } => Some(code),
            _ => None,
        })
        .await;
        let limited = refused == "rate_limited";
        refusals.push(refused);
        if limited {
            break;
        }
    }
    assert!(
        refusals.contains(&"rate_limited".to_owned()),
        "{refusals:?}"
    );
    // Friends lists need a login; acting needs our Origin.
    assert_eq!(request(addr, "GET", "/api/friends", "").await.status, 401);
    let no_origin = request(
        addr,
        "POST",
        "/api/friends/remove",
        &format!("Cookie: __Host-arena_session={alice}\r\n"),
    )
    .await;
    assert_eq!(no_origin.status, 403);
    door.stop().await.unwrap();
}

#[tokio::test]
async fn a_connection_logged_out_everywhere_cannot_invite() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let (door, _) = door_tuned(
        Origins::Only(vec!["https://arena.example.com".into()]),
        provider,
        |config| config.auth_burst = 1000,
    )
    .await;
    let addr = door.addr;
    let alice = log_in_as(addr, &fake, "alice").await;
    let bob = log_in_as(addr, &fake, "bob").await;
    let alice_h = choose(addr, &alice, "Alicex").await.1["handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let bob_h = choose(addr, &bob, "Bobx").await.1["handle"]
        .as_str()
        .unwrap()
        .to_owned();
    friend(addr, &alice, "request", &bob_h).await;
    friend(addr, &bob, "accept", &alice_h).await;
    let mut bob_socket = connect_queue(addr, 2, &bob).await;
    let mut alice_socket = connect_queue(addr, 1, &alice).await;
    // Logging out everywhere (from another device) leaves this tab open,
    // but it no longer acts for the account.
    let ended = request(
        addr,
        "POST",
        "/api/auth/logout_everywhere",
        &format!("Cookie: __Host-arena_session={alice}\r\n{ORIGIN}"),
    )
    .await;
    assert_eq!(ended.status, 200, "with WorkOS's logout link");
    say(
        &mut alice_socket,
        format!(r#"{{"type":"invite","handle":"{bob_h}"}}"#),
    )
    .await;
    let refused = until(&mut alice_socket, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(refused, "not_logged_in");
    assert!(
        !hears(&mut bob_socket, 200, |m| matches!(
            m,
            QueueMessage::Invite { .. }
        ))
        .await
    );
    door.stop().await.unwrap();
}

#[tokio::test]
async fn admins_come_from_configuration_and_alone_may_store_maps() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let directory = std::env::temp_dir().join(format!(
        "arena-admin-{}",
        arena_server::ticket::random_u64()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    // Both doors share one database, as a restart would.
    let database = test_database().await.unwrap();
    let key = SessionKey::random();
    let only = || Origins::Only(vec!["https://arena.example.com".into()]);
    let configure = |admins: HashSet<u64>| {
        let database = database.clone();
        let key = key.clone();
        let maps = directory.join("maps");
        move |config: &mut DoorConfig| {
            config.maps = Some(maps);
            let identity = config.identity.as_mut().unwrap();
            identity.database = database;
            identity.key = key;
            identity.admins = admins;
            config.auth_burst = 1000;
        }
    };
    // Accounts exist first; the operator then names one in the config.
    let (first, _) = door_tuned(only(), provider, configure(HashSet::new())).await;
    let alice = log_in_as(first.addr, &fake, "alice").await;
    let bob = log_in_as(first.addr, &fake, "bob").await;
    let alice_id = me(first.addr, &alice).await.account.unwrap();
    assert!(!me(first.addr, &alice).await.admin);
    first.stop().await.unwrap();
    let admins = HashSet::from([u64::from_str_radix(&alice_id, 16).unwrap()]);
    let (door, ticket_key) = door_tuned(only(), provider, configure(admins)).await;
    let addr = door.addr;
    assert!(me(addr, &alice).await.admin);
    assert!(!me(addr, &bob).await.admin);

    let as_cookie = |session: &str| format!("Cookie: __Host-arena_session={session}\r\n");
    let put = |session: String, id: &'static str, body: String, origin: bool| async move {
        let origin = if origin { ORIGIN } else { "" };
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "PUT /api/admin/maps/{id}.json HTTP/1.1\r\nHost: test\r\nCookie: __Host-arena_session={session}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        let status: u16 = response[9..12].parse().unwrap();
        let json = response.split_once("\r\n\r\n").unwrap().1;
        (
            status,
            serde_json::from_str::<serde_json::Value>(json).unwrap_or(serde_json::Value::Null),
        )
    };
    // Only the admin reaches the tool, on the server.
    let listed = request(addr, "GET", "/api/admin/maps", &as_cookie(&alice)).await;
    assert_eq!(
        (listed.status, listed.body.as_str()),
        (200, r#"{"maps":[]}"#)
    );
    let refused = request(addr, "GET", "/api/admin/maps", &as_cookie(&bob)).await;
    assert_eq!(
        (refused.status, refused.body.as_str()),
        (403, r#"{"error":"not_admin"}"#)
    );
    assert_eq!(
        request(addr, "GET", "/api/admin/maps", "").await.status,
        401
    );

    let map = arena_map::colosseum().to_json();
    assert_eq!(put(bob.clone(), "pit", map.clone(), true).await.0, 403);
    assert_eq!(
        put(alice.clone(), "pit", map.clone(), false).await.0,
        403,
        "writes need our Origin"
    );
    let (status, saved) = put(alice.clone(), "pit", map.clone(), true).await;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["id"], "pit");
    let (status, invalid) = put(alice.clone(), "broken", r#"{"format":1}"#.into(), true).await;
    assert_eq!(
        (status, invalid["error"].as_str()),
        (422, Some("invalid_map"))
    );
    assert!(!invalid["problems"].as_array().unwrap().is_empty());
    assert_eq!(put(alice.clone(), "Bad", map.clone(), true).await.0, 422);
    // No way out of the maps directory, and no oversized bodies.
    for path in [
        "/api/admin/maps/..%2Fsecret.json",
        "/api/admin/maps/..%2F..%2Fx.json",
    ] {
        let reply = request(addr, "GET", path, &as_cookie(&alice)).await;
        assert_eq!(reply.status, 404, "{path}");
    }
    assert_eq!(
        put(alice.clone(), "huge", "x".repeat(300 * 1024), true)
            .await
            .0,
        413
    );
    let back = request(addr, "GET", "/api/admin/maps/pit.json", &as_cookie(&alice)).await;
    assert_eq!(back.status, 200);
    assert_eq!(
        arena_map::parse(&back.body).unwrap().name,
        arena_map::colosseum().name
    );
    assert!(directory.join("maps/pit.json").exists());
    assert!(!directory.join("maps/broken.json").exists());

    // An admin can clear a player's handle; nobody else can.
    let bob_h = choose(addr, &bob, "Bobx").await.1["handle"]
        .as_str()
        .unwrap()
        .to_owned();
    let clear = |session: String, handle: String, origin: bool| async move {
        let origin = if origin { ORIGIN } else { "" };
        let body = format!(r#"{{"handle":"{handle}"}}"#);
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "POST /api/admin/handles/clear HTTP/1.1\r\nHost: test\r\nCookie: __Host-arena_session={session}\r\n{origin}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response[9..12].parse::<u16>().unwrap()
    };
    assert_eq!(clear(bob.clone(), bob_h.clone(), true).await, 403);
    assert_eq!(clear(alice.clone(), bob_h.clone(), false).await, 403);
    assert_eq!(clear(alice.clone(), "Nobody#0001".into(), true).await, 404);
    assert_eq!(clear(alice.clone(), bob_h.clone(), true).await, 204);
    let bob_now = me(addr, &bob).await;
    assert_eq!((bob_now.handle, bob_now.rename_at), (None, None));
    assert_eq!(
        choose(addr, &bob, "Bobby").await.0,
        200,
        "picks again at once"
    );

    // The admin flag rides in the admin's ticket, for the tag; not Bob's.
    let (alice_ticket, bob_ticket) = tokio::join!(
        queue_ticket(addr, 1, Some(&alice)),
        queue_ticket(addr, 2, Some(&bob))
    );
    let claims = |ticket: &str| ticket_key.verify(&from_hex(ticket).unwrap(), 0).unwrap();
    assert!(claims(&alice_ticket).admin);
    assert!(!claims(&bob_ticket).admin);
    door.stop().await.unwrap();
    let _ = std::fs::remove_dir_all(directory);
}

/// The next chat message: (channel, from, to, admin, text).
async fn chat_heard(socket: &mut Socket) -> (String, String, Option<String>, bool, String) {
    until(socket, |m| match m {
        QueueMessage::Chat {
            channel,
            from,
            to,
            admin,
            text,
        } => Some((channel, from, to, admin, text)),
        _ => None,
    })
    .await
}

async fn chat_refused(socket: &mut Socket) -> String {
    until(socket, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn whispers_reach_friends_and_admins_mute_while_blocks_silence() {
    if no_database() {
        return;
    }
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let directory =
        std::env::temp_dir().join(format!("arena-chat-{}", arena_server::ticket::random_u64()));
    std::fs::create_dir_all(&directory).unwrap();
    // Both doors share one database, as a restart would.
    let database = test_database().await.unwrap();
    let key = SessionKey::random();
    let only = || Origins::Only(vec!["https://arena.example.com".into()]);
    let configure = |admins: HashSet<u64>| {
        let database = database.clone();
        let key = key.clone();
        let maps = directory.join("maps");
        move |config: &mut DoorConfig| {
            config.maps = Some(maps);
            let identity = config.identity.as_mut().unwrap();
            identity.database = database;
            identity.key = key;
            identity.admins = admins;
            config.auth_burst = 1000;
        }
    };
    let (first, _) = door_tuned(only(), provider, configure(HashSet::new())).await;
    let alice = log_in_as(first.addr, &fake, "alice").await;
    let alice_id = me(first.addr, &alice).await.account.unwrap();
    first.stop().await.unwrap();
    let admins = HashSet::from([u64::from_str_radix(&alice_id, 16).unwrap()]);
    let (door, _) = door_tuned(only(), provider, configure(admins)).await;
    let addr = door.addr;
    let bob = log_in_as(addr, &fake, "bob").await;
    let carol = log_in_as(addr, &fake, "carol").await;
    let mut handles = Vec::new();
    for (session, name) in [(&alice, "Alicex"), (&bob, "Bobx"), (&carol, "Carolx")] {
        handles.push(
            choose(addr, session, name).await.1["handle"]
                .as_str()
                .unwrap()
                .to_owned(),
        );
    }
    let [alice_h, bob_h, carol_h] = <[String; 3]>::try_from(handles).unwrap();
    friend(addr, &alice, "request", &bob_h).await;
    friend(addr, &bob, "accept", &alice_h).await;
    let mut alice_socket = connect_queue(addr, 1, &alice).await;
    let mut bob_socket = connect_queue(addr, 2, &bob).await;
    let mut carol_socket = connect_queue(addr, 3, &carol).await;
    let mut guest_socket = connect_queue(addr, 4, "nobody").await;

    // A whisper reaches the friend, and the sender sees who it went to;
    // the admin's tag comes from the door.
    say(
        &mut alice_socket,
        serde_json::json!({"type": "whisper", "to": bob_h, "text": "duo?"}).to_string(),
    )
    .await;
    assert_eq!(
        chat_heard(&mut bob_socket).await,
        ("whisper".into(), alice_h.clone(), None, true, "duo?".into())
    );
    assert_eq!(
        chat_heard(&mut alice_socket).await,
        (
            "whisper".into(),
            alice_h.clone(),
            Some(bob_h.clone()),
            true,
            "duo?".into()
        )
    );
    assert!(
        !hears(&mut carol_socket, 200, |m| matches!(
            m,
            QueueMessage::Chat { .. }
        ))
        .await
    );
    // Strangers and guests cannot whisper.
    say(
        &mut alice_socket,
        serde_json::json!({"type": "whisper", "to": carol_h, "text": "hi"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut alice_socket).await, "no_such_friend");
    say(
        &mut guest_socket,
        serde_json::json!({"type": "whisper", "to": bob_h, "text": "hi"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut guest_socket).await, "not_logged_in");

    // An admin mutes Bob; a non-admin cannot mute anyone.
    let mute = |session: String, handle: String, minutes: u32| async move {
        let body = format!(r#"{{"handle":"{handle}","minutes":{minutes}}}"#);
        let mut stream = TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "POST /api/admin/mute HTTP/1.1\r\nHost: test\r\nCookie: __Host-arena_session={session}\r\n{ORIGIN}Content-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).await.unwrap();
        response[9..12].parse::<u16>().unwrap()
    };
    assert_eq!(mute(bob.clone(), alice_h.clone(), 10).await, 403);
    assert_eq!(mute(alice.clone(), bob_h.clone(), 10).await, 204);
    say(
        &mut bob_socket,
        serde_json::json!({"type": "chat", "channel": "public", "text": "hey"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut bob_socket).await, "muted");
    assert_eq!(mute(alice.clone(), bob_h.clone(), 0).await, 204);
    say(
        &mut bob_socket,
        serde_json::json!({"type": "chat", "channel": "public", "text": "hey"}).to_string(),
    )
    .await;
    assert_eq!(chat_heard(&mut carol_socket).await.4, "hey");
    assert_eq!(
        chat_heard(&mut bob_socket).await.4,
        "hey",
        "senders hear their own post"
    );

    // Carol blocks Alice: Alice's chat stops reaching Carol's open tab.
    assert_eq!(friend(addr, &carol, "block", &alice_h).await.0, 204);
    tokio::time::sleep(Duration::from_millis(50)).await;
    say(
        &mut alice_socket,
        serde_json::json!({"type": "chat", "channel": "public", "text": "anyone?"}).to_string(),
    )
    .await;
    assert_eq!(chat_heard(&mut bob_socket).await.4, "anyone?");
    assert!(
        !hears(&mut carol_socket, 200, |m| matches!(
            m,
            QueueMessage::Chat { .. }
        ))
        .await
    );

    // Guests cannot post in public.
    say(
        &mut guest_socket,
        serde_json::json!({"type": "chat", "channel": "public", "text": "hi"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut guest_socket).await, "log_in");

    // Bob and Carol play while Alice, Bob's friend, watches: no channel
    // and no whisper of hers reaches them.
    say(
        &mut bob_socket,
        r#"{"type":"create_lobby","mode":"1v1"}"#.into(),
    )
    .await;
    let code = until(&mut bob_socket, |m| match m {
        QueueMessage::Lobby(view) => Some(view.code),
        _ => None,
    })
    .await;
    for socket in [&mut carol_socket, &mut alice_socket] {
        say(
            socket,
            format!(r#"{{"type":"join_lobby","code":"{code}"}}"#),
        )
        .await;
        until(socket, |m| {
            matches!(m, QueueMessage::Lobby(_)).then_some(())
        })
        .await;
    }
    // The lobby names its members by their handles.
    let names = until(&mut bob_socket, |m| match m {
        QueueMessage::Lobby(view) if view.members.len() == 3 => Some(
            view.members
                .into_iter()
                .map(|member| member.handle)
                .collect::<Vec<_>>(),
        ),
        _ => None,
    })
    .await;
    assert_eq!(
        names,
        [
            Some(bob_h.clone()),
            Some(carol_h.clone()),
            Some(alice_h.clone())
        ]
    );
    // A new name (here only its letter case, which is allowed at once)
    // reaches lobby-mates without waiting for another lobby change.
    let upper = bob_h.to_uppercase();
    assert_eq!(
        choose(addr, &bob, &upper[..upper.find('#').unwrap()])
            .await
            .0,
        200
    );
    until(&mut carol_socket, |m| match m {
        QueueMessage::Lobby(view) => view
            .members
            .iter()
            .any(|member| member.handle.as_deref() == Some(upper.as_str()))
            .then_some(()),
        _ => None,
    })
    .await;
    // And back, so the rest of the test whispers to the same handle.
    assert_eq!(
        choose(addr, &bob, &bob_h[..bob_h.find('#').unwrap()])
            .await
            .0,
        200
    );
    say(&mut bob_socket, r#"{"type":"start_lobby"}"#.into()).await;
    for socket in [&mut bob_socket, &mut carol_socket, &mut alice_socket] {
        until(socket, |m| {
            matches!(m, QueueMessage::Match { .. }).then_some(())
        })
        .await;
    }
    tokio::time::sleep(Duration::from_secs(1)).await;
    say(
        &mut alice_socket,
        serde_json::json!({"type": "whisper", "to": bob_h, "text": "carol is low"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut alice_socket).await, "spectators_only");
    // A second tab of hers is not spectating, but players in a live match
    // hear neither public chat nor whispers.
    let mut alice_tab = connect_queue(addr, 5, &alice).await;
    say(
        &mut alice_tab,
        serde_json::json!({"type": "chat", "channel": "public", "text": "carol is low"})
            .to_string(),
    )
    .await;
    assert_eq!(chat_heard(&mut alice_tab).await.4, "carol is low");
    say(
        &mut alice_tab,
        serde_json::json!({"type": "whisper", "to": bob_h, "text": "carol is low"}).to_string(),
    )
    .await;
    assert_eq!(chat_refused(&mut alice_tab).await, "in_match");
    for player in [&mut bob_socket, &mut carol_socket] {
        assert!(!hears(player, 200, |m| matches!(m, QueueMessage::Chat { .. })).await);
    }
    door.stop().await.unwrap();
    let _ = std::fs::remove_dir_all(directory);
}
