//! Accounts end to end (ADR 0055) against a fake OAuth provider on
//! loopback: login with state and PKCE, the session cookie, the account id
//! in queue tickets, and every way a cookie must stop working.

use std::collections::{HashMap, HashSet};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arena_server::door::{self, DoorConfig, Me, QueueMessage};
use arena_server::identity::IdentityConfig;
use arena_server::identity::cookie::{SessionKey, pkce_challenge};
use arena_server::identity::provider::Provider;
use arena_server::identity::store::ProviderId;
use arena_server::ingress::Origins;
use arena_server::ticket::{Claims, TicketKey, from_hex};
use axum::extract::{Form, State};
use axum::http::HeaderMap;
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const PAGE: &str = "https://arena.example.com/";
const ORIGIN: &str = "Origin: https://arena.example.com\r\n";
const SUBJECT: &str = "80351110224678912";
const WAIT: Duration = Duration::from_secs(5);

/// The challenge the fake provider expects the verifier to match.
#[derive(Clone, Default)]
struct Fake(Arc<Mutex<Option<String>>>);

async fn token(
    State(fake): State<Fake>,
    Form(form): Form<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let expected = fake.0.lock().unwrap().clone();
    let ok = form.get("code").map(String::as_str) == Some("good-code")
        && form.get("grant_type").map(String::as_str) == Some("authorization_code")
        && form.get("client_secret").map(String::as_str) == Some("shh")
        && expected.is_some()
        && form
            .get("code_verifier")
            .map(|verifier| pkce_challenge(verifier))
            == expected;
    Json(if ok {
        serde_json::json!({"access_token": "tok", "token_type": "Bearer"})
    } else {
        serde_json::json!({"error": "invalid_grant"})
    })
}

async fn user(headers: HeaderMap) -> Json<serde_json::Value> {
    let bearer = headers.get("authorization").and_then(|v| v.to_str().ok());
    Json(if bearer == Some("Bearer tok") {
        serde_json::json!({"id": SUBJECT, "username": "someone"})
    } else {
        serde_json::json!({})
    })
}

async fn fake_provider(fake: Fake) -> SocketAddr {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let app = Router::new()
        .route("/token", post(token))
        .route("/user", get(user))
        .with_state(fake);
    tokio::spawn(async move { axum::serve(listener, app).await });
    addr
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
    let key = TicketKey::random();
    let mut discord = Provider::new(ProviderId::Discord, "client".into(), "shh".into());
    discord.authorize_url = format!("http://{provider}/authorize");
    discord.token_url = format!("http://{provider}/token");
    discord.user_url = format!("http://{provider}/user");
    // GitHub is configured too, to prove a Discord login cannot finish there.
    let mut github = Provider::new(ProviderId::Github, "client".into(), "shh".into());
    github.token_url = format!("http://{provider}/token");
    github.user_url = format!("http://{provider}/user");
    let mut config = DoorConfig::new(key.clone(), "ws://127.0.0.1:1/play".into(), PAGE.into());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.origins = origins;
    config.identity = Some(IdentityConfig {
        store: None,
        key: SessionKey::random(),
        providers: vec![discord, github],
        admins: HashSet::new(),
    });
    (door::start(config).await.unwrap(), key)
}

/// Starts a Discord login; returns the login cookie and the provider link.
async fn start_login(addr: SocketAddr) -> (String, reqwest::Url) {
    let started = request(addr, "GET", "/api/auth/discord/login", "").await;
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
    let (oauth, link) = start_login(addr).await;
    let query = query(&link);
    assert_eq!(query["scope"], "identify");
    assert_eq!(query["code_challenge_method"], "S256");
    assert_eq!(
        query["redirect_uri"],
        "https://arena.example.com/api/auth/discord/callback"
    );
    *fake.0.lock().unwrap() = Some(query["code_challenge"].clone());
    let back = request(
        addr,
        "GET",
        &format!(
            "/api/auth/discord/callback?code=good-code&state={}",
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
    let fake = Fake::default();
    let provider = fake_provider(fake.clone()).await;
    let (door, key) = door_with(
        Origins::Only(vec!["https://arena.example.com".into()]),
        provider,
    )
    .await;
    let addr = door.addr;
    let claims = |ticket: &str| -> Claims { key.verify(&from_hex(ticket).unwrap(), 0).unwrap() };

    // A guest: no account, and the providers on offer.
    let guest: Me =
        serde_json::from_str(&request(addr, "GET", "/api/auth/me", "").await.body).unwrap();
    assert_eq!(
        guest,
        Me {
            account: None,
            admin: false,
            providers: vec!["discord".into(), "github".into()]
        }
    );
    assert_eq!(
        request(addr, "GET", "/api/auth/google/login", "")
            .await
            .status,
        404,
        "not configured"
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
        "/api/auth/discord/callback?code=good-code&state=someone-elses",
        &format!("Cookie: __Host-arena_oauth={oauth}\r\n"),
    )
    .await;
    assert_eq!(forged.status, 400);
    assert!(forged.cookie("__Host-arena_session").is_none());
    // Nor does a callback without a login started, nor a Discord login
    // finished at another provider's callback.
    let unstarted = request(
        addr,
        "GET",
        "/api/auth/discord/callback?code=good-code&state=x",
        "",
    )
    .await;
    assert_eq!(unstarted.status, 400);
    let elsewhere = request(
        addr,
        "GET",
        &format!(
            "/api/auth/github/callback?code=good-code&state={}",
            query(&link)["state"]
        ),
        &format!("Cookie: __Host-arena_oauth={oauth}\r\n"),
    )
    .await;
    assert_eq!(elsewhere.status, 400);
    // A tampered session is a guest.
    let tampered = format!("{}0", &session[..session.len() - 1]);
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
    // Logging out everywhere ends this session and any copy of it.
    let other_device = log_in(addr, &fake).await;
    let ended = request(
        addr,
        "POST",
        "/api/auth/logout_everywhere",
        &format!("{cookie}{ORIGIN}"),
    )
    .await;
    assert_eq!(ended.status, 204);
    assert_eq!(ended.cookie("__Host-arena_session").unwrap().1, "0");
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
    for _ in 0..30 {
        statuses.push(
            request(addr, "GET", "/api/auth/discord/login", "")
                .await
                .status,
        );
    }
    assert!(statuses.contains(&429), "{statuses:?}");
    door.stop().await.unwrap();
}

#[tokio::test]
async fn cookies_never_act_for_a_door_open_to_any_origin() {
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
