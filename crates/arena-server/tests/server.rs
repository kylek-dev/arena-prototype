//! End-to-end tests over real WebSockets against a server on loopback.
//! Ticks are shortened to 5 ms; the rules still count 30 ticks per second.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{
    ClientMessage, ErrorCode, FighterState, InputFrame, PROTOCOL_VERSION, ServerMessage, Snapshot,
    Welcome, WireEvent, WireRejection, WireResult,
};
use arena_server::ticket::{Claims, TicketKey, from_hex};
use arena_server::{Config, NewMatch, Running, start};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const WAIT: Duration = Duration::from_secs(5);

fn config(dev: bool) -> Config {
    let mut config = Config::new(TicketKey::random());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.dev = dev;
    config.tick = Duration::from_millis(5);
    config.hello_timeout = Duration::from_millis(300);
    config
}

async fn server() -> Running {
    start(config(true)).await.unwrap()
}

/// A minimal HTTP/1.1 request; returns the status and body.
async fn http(addr: SocketAddr, method: &str, path: &str) -> (u16, String) {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let status = response[9..12].parse().unwrap();
    let body = response.split_once("\r\n\r\n").unwrap().1.to_owned();
    (status, body)
}

async fn new_match(addr: SocketAddr, team_size: usize) -> NewMatch {
    let (status, body) = http(addr, "POST", &format!("/dev/match?team_size={team_size}")).await;
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// A test client whose reader drains the socket continuously, so the
/// server never sees it as slow. `None` in the stream means closed.
struct Client {
    sink: SplitSink<Socket, Message>,
    inbox: mpsc::UnboundedReceiver<ServerMessage>,
    seq: u32,
}
impl Client {
    async fn open(addr: SocketAddr) -> Self {
        let (socket, _) = connect_async(format!("ws://{addr}/play")).await.unwrap();
        let (sink, mut stream) = socket.split();
        let (sender, inbox) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                match message {
                    Message::Binary(frame) => {
                        let message = ServerMessage::decode(&frame).expect("server frames decode");
                        if sender.send(message).is_err() {
                            break;
                        }
                    }
                    Message::Close(_) => break,
                    _ => {}
                }
            }
        });
        Self {
            sink,
            inbox,
            seq: 0,
        }
    }
    /// Closes the socket as a browser tab would when it goes away.
    async fn leave(mut self) {
        let _ = self.sink.close().await;
    }
    async fn raw(&mut self, message: Message) {
        let _ = self.sink.send(message).await;
    }
    async fn send(&mut self, message: &ClientMessage) {
        self.raw(Message::binary(message.encode())).await;
    }
    async fn hello(&mut self, ticket: Vec<u8>) {
        self.send(&ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket,
            appearance: None,
        })
        .await;
    }
    async fn join(addr: SocketAddr, ticket: &str) -> (Self, Welcome) {
        let mut client = Self::open(addr).await;
        client.hello(from_hex(ticket).unwrap()).await;
        match client.next().await {
            Some(ServerMessage::Welcome(welcome)) => (client, welcome),
            other => panic!("expected Welcome, got {other:?}"),
        }
    }
    async fn input(&mut self, frame: InputFrame) {
        self.seq += 1;
        self.send(&ClientMessage::Input(InputFrame {
            seq: self.seq,
            ..frame
        }))
        .await;
    }
    async fn next(&mut self) -> Option<ServerMessage> {
        tokio::time::timeout(WAIT, self.inbox.recv())
            .await
            .expect("server answered in time")
    }
    /// Reads until an `Error`, which must be followed by the close.
    async fn error(&mut self) -> ErrorCode {
        loop {
            match self.next().await {
                Some(ServerMessage::Error(code)) => {
                    assert_eq!(self.next().await, None, "closed after the error");
                    return code;
                }
                Some(_) => continue,
                None => panic!("closed without an error message"),
            }
        }
    }
    async fn closed(&mut self) -> bool {
        loop {
            match self.next().await {
                Some(ServerMessage::Error(code)) => panic!("unexpected {code:?}"),
                Some(_) => continue,
                None => return true,
            }
        }
    }
    async fn snapshot_until(&mut self, done: impl Fn(&Snapshot) -> bool) -> Snapshot {
        loop {
            match self.next().await {
                Some(ServerMessage::Snapshot(snapshot)) if done(&snapshot) => return snapshot,
                Some(_) => continue,
                None => panic!("closed while waiting for a snapshot"),
            }
        }
    }
}

fn forward() -> InputFrame {
    InputFrame {
        movement: [0, -1000],
        ..InputFrame::default()
    }
}

#[tokio::test]
async fn two_players_play_a_match_the_server_decides() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    assert_eq!(created.tickets.len(), 2);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    assert!(created.expires > now + 3_600, "tickets outlive a match");
    let (mut first, welcome) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(
        (welcome.actor, welcome.team_size, welcome.protocol),
        (0, 1, PROTOCOL_VERSION)
    );
    let spawn = first.snapshot_until(|_| true).await.fighters;
    // Alone in the match: inputs never act.
    first.input(forward()).await;
    let waiting = first.snapshot_until(|snapshot| snapshot.ack == 1).await;
    assert_eq!((waiting.tick, &waiting.fighters), (0, &spawn));

    let (mut second, welcome) = Client::join(addr, &created.tickets[1]).await;
    assert_eq!(welcome.actor, 1);
    // Actor 0 walks; only actor 0 moves, as far as the rules allow.
    let mut moved = waiting;
    while moved.fighters[0].z > spawn[0].z - 10 * 240 {
        first.input(forward()).await;
        let seq = first.seq;
        moved = first.snapshot_until(|snapshot| snapshot.ack == seq).await;
    }
    assert!(moved.tick > 0);
    assert_eq!(
        FighterState {
            connected: false,
            ..moved.fighters[1]
        },
        spawn[1],
        "the other seat never moved"
    );
    assert!(spawn[0].connected && !spawn[1].connected);
    assert!(moved.fighters.iter().all(|fighter| fighter.connected));
    // Actor 1 attacks from out of range: refused, and only actor 1 hears so.
    second
        .input(InputFrame {
            action: InputFrame::pulse(0),
            ..InputFrame::default()
        })
        .await;
    let refused = second
        .snapshot_until(|snapshot| {
            snapshot
                .events
                .iter()
                .any(|event| event.event == WireEvent::Rejected(WireRejection::OutOfRange))
        })
        .await;
    assert_eq!(refused.fighters[0].health, 100);
    let tick = refused.tick;
    let seen_by_first = first.snapshot_until(|snapshot| snapshot.tick >= tick).await;
    assert!(
        !seen_by_first
            .events
            .iter()
            .any(|event| matches!(event.event, WireEvent::Rejected(_)))
    );
    // Ping is answered directly.
    first.send(&ClientMessage::Ping { nonce: 77 }).await;
    loop {
        match first.next().await {
            Some(ServerMessage::Pong { nonce }) => {
                assert_eq!(nonce, 77);
                break;
            }
            Some(_) => continue,
            None => panic!("closed"),
        }
    }
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn hostile_connections_get_the_right_error_and_never_a_seat() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let key = running.app.key().clone();
    let valid = from_hex(&created.tickets[0]).unwrap();
    let signed = |claims| key.issue(&claims);
    let far_future = u64::MAX;

    let mut tampered = valid.clone();
    *tampered.last_mut().unwrap() ^= 1;
    let cases: Vec<(&str, Message, ErrorCode)> = vec![
        (
            "tampered ticket",
            Message::binary(hello(tampered)),
            ErrorCode::BadTicket,
        ),
        (
            "expired ticket",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 0 },
                team_size: 1,
                expires: 1,
                settings: None,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "seat outside the match",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 2 },
                team_size: 1,
                expires: far_future,
                settings: None,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "team size the rules cannot run",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id ^ 1,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 0 },
                team_size: 4,
                expires: far_future,
                settings: None,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "invalid custom settings version",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id ^ 2,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 0 },
                team_size: 1,
                expires: far_future,
                settings: Some(arena_server::ticket::MatchSettings {
                    version: arena_server::ticket::MATCH_SETTINGS_VERSION + 1,
                    map: arena_server::ticket::MapId::Colosseum,
                    round_seconds: 600,
                    allow_spectators: true,
                }),
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "non-allowlisted custom round limit",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id ^ 3,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 0 },
                team_size: 1,
                expires: far_future,
                settings: Some(arena_server::ticket::MatchSettings {
                    version: arena_server::ticket::MATCH_SETTINGS_VERSION,
                    map: arena_server::ticket::MapId::Colosseum,
                    round_seconds: 123,
                    allow_spectators: true,
                }),
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "spectator ticket when spectators are disabled",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id ^ 4,
                player: 1,
                role: arena_server::ticket::Role::Spectator,
                team_size: 1,
                expires: far_future,
                settings: Some(arena_server::ticket::MatchSettings {
                    version: arena_server::ticket::MATCH_SETTINGS_VERSION,
                    map: arena_server::ticket::MapId::Colosseum,
                    round_seconds: 600,
                    allow_spectators: false,
                }),
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "another ticket key",
            Message::binary(hello(TicketKey::random().issue(&Claims {
                match_id: created.match_id,
                player: 1,
                role: arena_server::ticket::Role::Player { seat: 0 },
                team_size: 1,
                expires: far_future,
                settings: None,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "future protocol",
            Message::binary(
                ClientMessage::Hello {
                    protocol: PROTOCOL_VERSION + 1,
                    ticket: valid.clone(),
                    appearance: None,
                }
                .encode(),
            ),
            ErrorCode::VersionMismatch,
        ),
        ("text frame", Message::text("hello"), ErrorCode::Malformed),
        (
            "garbage",
            Message::binary(vec![9, 9, 9]),
            ErrorCode::Malformed,
        ),
        (
            "input before hello",
            Message::binary(ClientMessage::Input(InputFrame::default()).encode()),
            ErrorCode::Malformed,
        ),
    ];
    for (name, first, expected) in cases {
        let mut client = Client::open(addr).await;
        client.raw(first).await;
        assert_eq!(client.error().await, expected, "{name}");
    }

    // Silence is timed out.
    let mut silent = Client::open(addr).await;
    assert_eq!(silent.error().await, ErrorCode::HelloTimeout);
    // An oversized frame is cut off by the transport before decoding.
    let mut huge = Client::open(addr).await;
    huge.raw(Message::binary(vec![
        0;
        arena_protocol::MAX_CLIENT_FRAME + 1
    ]))
    .await;
    assert!(huge.closed().await);
    // A second Hello after joining is a protocol violation.
    let (mut joined, _) = Client::join(addr, &created.tickets[0]).await;
    joined.hello(valid).await;
    assert_eq!(joined.error().await, ErrorCode::Malformed);

    let metrics = running.app.metrics();
    assert_eq!(metrics.errors(ErrorCode::BadTicket), 8);
    assert_eq!(metrics.errors(ErrorCode::Malformed), 4);
    assert_eq!(metrics.errors(ErrorCode::HelloTimeout), 1);
    // Refused tickets never start a match; only the valid one did.
    assert_eq!(
        arena_server::metrics::Metrics::get(&metrics.matches_created),
        1
    );
    running.shut_down(Duration::ZERO).await.unwrap();
}

fn hello(ticket: Vec<u8>) -> Vec<u8> {
    ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket,
        appearance: None,
    }
    .encode()
}

#[tokio::test]
async fn flooding_connections_are_rate_limited() {
    let running = server().await;
    let created = new_match(running.addr, 1).await;
    let (mut client, _) = Client::join(running.addr, &created.tickets[0]).await;
    for nonce in 0..200 {
        client.send(&ClientMessage::Ping { nonce }).await;
    }
    assert_eq!(client.error().await, ErrorCode::RateLimited);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn a_reconnect_takes_over_the_seat() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut old, _) = Client::join(addr, &created.tickets[0]).await;
    let (_other, _) = Client::join(addr, &created.tickets[1]).await;
    let (mut new, welcome) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome.actor, 0);
    assert_eq!(
        old.error().await,
        ErrorCode::Replaced,
        "the replaced socket is told why, so it does not take the seat back"
    );
    // Inputs before the match runs are acknowledged without acting, so
    // wait for it to run.
    let start = new
        .snapshot_until(|snapshot| snapshot.tick > 0)
        .await
        .fighters[0];
    // The new connection's sequence starts again at 1.
    new.input(forward()).await;
    let moved = new.snapshot_until(|snapshot| snapshot.ack == 1).await;
    assert!(moved.fighters[0].z < start.z);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn operations_endpoints_and_development_guards() {
    let running = server().await;
    let addr = running.addr;
    assert_eq!(http(addr, "GET", "/healthz").await, (200, "ok".into()));
    assert_eq!(http(addr, "GET", "/readyz").await, (200, "ready".into()));
    assert_eq!(http(addr, "POST", "/dev/match?team_size=7").await.0, 400);
    let created = new_match(addr, 3).await;
    // Issuing tickets starts nothing; the first player to connect does.
    let (_, metrics) = http(addr, "GET", "/metrics").await;
    assert!(metrics.contains("arena_matches_active 0\n"), "{metrics}");
    let (_first, _) = Client::join(addr, &created.tickets[4]).await;
    let (status, metrics) = http(addr, "GET", "/metrics").await;
    assert_eq!(status, 200);
    assert!(
        metrics.contains("arena_matches_created_total 1\n"),
        "{metrics}"
    );
    assert!(metrics.contains("arena_matches_active 1\n"), "{metrics}");
    running.shut_down(Duration::ZERO).await.unwrap();

    // Without --dev there is no ticket issuing endpoint.
    let production = start(config(false)).await.unwrap();
    assert_eq!(http(production.addr, "POST", "/dev/match").await.0, 404);
    production.shut_down(Duration::ZERO).await.unwrap();

    // Development mode refuses to listen beyond loopback.
    let mut exposed = config(true);
    exposed.bind = SocketAddr::from(([0, 0, 0, 0], 0));
    assert!(start(exposed).await.is_err());
}

#[tokio::test]
async fn shutting_down_tells_every_player_and_refuses_new_matches() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut second, _) = Client::join(addr, &created.tickets[1]).await;
    let app = running.app.clone();
    let stopping = tokio::spawn(running.shut_down(Duration::from_millis(200)));
    // While draining, readiness fails and no new match starts.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(app.is_draining());
    assert_eq!(http(addr, "GET", "/readyz").await.0, 503);
    assert_eq!(http(addr, "POST", "/dev/match").await.0, 503);
    assert_eq!(first.error().await, ErrorCode::ServerShutdown);
    assert_eq!(second.error().await, ErrorCode::ServerShutdown);
    stopping.await.unwrap().unwrap();
}

#[tokio::test]
async fn a_dropped_player_shows_as_disconnected_stands_idle_and_can_rejoin() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut second, _) = Client::join(addr, &created.tickets[1]).await;
    // Actor 0 walks a few steps, then their tab goes away mid-match.
    for _ in 0..5 {
        first.input(forward()).await;
    }
    let seq = first.seq;
    let walked = first.snapshot_until(|snapshot| snapshot.ack == seq).await;
    first.leave().await;
    let gone = second
        .snapshot_until(|snapshot| !snapshot.fighters[0].connected)
        .await;
    assert!(gone.fighters[1].connected);
    assert_eq!(gone.result, None, "a disconnect is never a forfeit");
    // The disconnected fighter stands idle where it was.
    let later = second
        .snapshot_until(|snapshot| snapshot.tick >= gone.tick + 30)
        .await;
    assert_eq!(
        (later.fighters[0].x, later.fighters[0].z),
        (walked.fighters[0].x, walked.fighters[0].z)
    );
    assert!(!later.fighters[0].connected);
    // Rejoining with the same ticket restores the seat and its state.
    let (mut back, welcome) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome.actor, 0);
    assert!(welcome.tick >= later.tick);
    let restored = back.snapshot_until(|_| true).await;
    assert!(restored.fighters[0].connected);
    assert_eq!(restored.fighters[0].health, walked.fighters[0].health);
    second
        .snapshot_until(|snapshot| snapshot.fighters[0].connected)
        .await;
    back.input(forward()).await;
    let moved = back.snapshot_until(|snapshot| snapshot.ack == 1).await;
    assert!(moved.fighters[0].z < walked.fighters[0].z);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn draining_keeps_running_matches_until_stopped() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, _) = Client::join(addr, &created.tickets[0]).await;
    let (_second, _) = Client::join(addr, &created.tickets[1]).await;
    running.begin_drain();
    assert_eq!(running.active_matches(), 1);
    // No new matches and not ready, but the running match plays on ...
    assert_eq!(http(addr, "GET", "/readyz").await.0, 503);
    assert_eq!(http(addr, "POST", "/dev/match").await.0, 503);
    let before = first
        .snapshot_until(|snapshot| snapshot.tick > 0)
        .await
        .tick;
    first
        .snapshot_until(|snapshot| snapshot.tick > before + 30)
        .await;
    // ... and its players may still rejoin.
    let (mut back, welcome) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome.actor, 0);
    assert!(!running.drained(Duration::from_millis(100)).await);
    // Stopping closes it at once, without waiting for the match to end.
    let stopped = tokio::time::Instant::now();
    running.stop().await.unwrap();
    assert_eq!(back.error().await, ErrorCode::ServerShutdown);
    assert!(stopped.elapsed() < Duration::from_secs(3));
}

#[tokio::test]
async fn a_match_starts_with_its_first_ticket_and_stays_over_once_ended() {
    let mut short = config(true);
    // An unfilled match ends after 3 rule ticks.
    short.limits.start_timeout = 3;
    let running = start(short).await.unwrap();
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    assert_eq!(running.active_matches(), 0);
    let (mut lonely, welcome) = Client::join(addr, &created.tickets[1]).await;
    assert_eq!((welcome.actor, welcome.team_size), (1, 1));
    assert_eq!(running.active_matches(), 1);
    // Nobody else came: the match ends ...
    assert_eq!(lonely.error().await, ErrorCode::MatchOver);
    assert!(running.drained(WAIT).await);
    // ... and its tickets cannot start it again.
    let mut late = Client::open(addr).await;
    late.hello(from_hex(&created.tickets[0]).unwrap()).await;
    assert_eq!(late.error().await, ErrorCode::MatchOver);
    assert_eq!(running.active_matches(), 0);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn a_draining_server_starts_no_match_but_keeps_hosting_its_own() {
    let running = server().await;
    let addr = running.addr;
    let hosted = new_match(addr, 1).await;
    let waiting = new_match(addr, 1).await;
    let (_first, _) = Client::join(addr, &hosted.tickets[0]).await;
    running.begin_drain();
    // A player of a match this server never started is told to retry
    // (`ServerShutdown`), not that the match is over.
    let mut newcomer = Client::open(addr).await;
    newcomer.hello(from_hex(&waiting.tickets[0]).unwrap()).await;
    assert_eq!(newcomer.error().await, ErrorCode::ServerShutdown);
    let (_second, welcome) = Client::join(addr, &hosted.tickets[1]).await;
    assert_eq!(welcome.actor, 1);
    assert_eq!(running.active_matches(), 1);
    running.stop().await.unwrap();
}

/// The handshake status of a WebSocket upgrade with extra headers.
async fn upgrade_status(addr: SocketAddr, headers: &[(&str, &str)]) -> u16 {
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    let mut request = format!("ws://{addr}/play").into_client_request().unwrap();
    for (name, value) in headers {
        request.headers_mut().insert(
            tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
    }
    match connect_async(request).await {
        Ok((_, response)) => response.status().as_u16(),
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => response.status().as_u16(),
        Err(error) => panic!("{error}"),
    }
}

#[tokio::test]
async fn only_allowed_pages_connect_and_each_address_holds_a_few_connections() {
    let mut guarded = config(false);
    guarded.origins =
        arena_server::ingress::Origins::Only(vec!["https://arena.example.com".into()]);
    guarded.connections_per_address = 2;
    // Connections from loopback are a proxy forwarding for players.
    guarded.trusted_proxies = vec![arena_server::ingress::Cidr::parse("127.0.0.1").unwrap()];
    let running = start(guarded).await.unwrap();
    let addr = running.addr;
    let page = ("origin", "https://arena.example.com");
    assert_eq!(upgrade_status(addr, &[page]).await, 101);
    assert_eq!(
        upgrade_status(addr, &[("origin", "https://elsewhere.example")]).await,
        403
    );
    assert_eq!(upgrade_status(addr, &[]).await, 101, "not a browser");
    // Each forwarded address may hold two open connections.
    let open = |client: &'static str| async move {
        use tokio_tungstenite::tungstenite::client::IntoClientRequest;
        let mut request = format!("ws://{addr}/play").into_client_request().unwrap();
        request
            .headers_mut()
            .insert("x-forwarded-for", client.parse().unwrap());
        connect_async(request).await
    };
    let first = open("198.51.100.1").await.unwrap();
    let _second = open("198.51.100.1").await.unwrap();
    match open("198.51.100.1").await {
        Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
            assert_eq!(response.status().as_u16(), 429)
        }
        other => panic!(
            "expected 429, got {:?}",
            other.map(|(_, response)| response.status())
        ),
    }
    assert!(open("198.51.100.2").await.is_ok(), "another player");
    // Closing one frees its place once the server sees it go.
    drop(first);
    let mut reopened = None;
    for _ in 0..100 {
        if let Ok(socket) = open("198.51.100.1").await {
            reopened = Some(socket);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(reopened.is_some());
    let (_, metrics) = http(addr, "GET", "/metrics").await;
    assert!(
        metrics.contains("arena_connections_refused_origin_total 1\n"),
        "{metrics}"
    );
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn a_round_that_reaches_the_time_limit_ends_in_a_time_limit_draw() {
    // Server startup rejects out-of-range round limits instead of failing at join time.
    for invalid in [0, arena_sim::arena::MAX_ROUND_TICKS + 1] {
        let mut bad = config(true);
        bad.round_ticks = invalid;
        assert!(start(bad).await.is_err(), "round_ticks={invalid}");
    }
    let mut short = config(true);
    short.round_ticks = 12;
    short.limits.linger = 3;
    let running = start(short).await.unwrap();
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, welcome_a) = Client::join(addr, &created.tickets[0]).await;
    let (mut second, welcome_b) = Client::join(addr, &created.tickets[1]).await;
    assert_eq!(welcome_a.round_ticks, 12);
    assert_eq!(welcome_b.round_ticks, 12);

    // Wait for both clients to observe the authoritative time-limit draw.
    let done_a = first
        .snapshot_until(|snapshot| snapshot.result.is_some())
        .await;
    let done_b = second
        .snapshot_until(|snapshot| snapshot.result.is_some())
        .await;
    assert_eq!(done_a.tick, 12);
    assert_eq!(done_b.tick, 12);
    assert_eq!(done_a.result, Some(WireResult::TimeLimitDraw));
    assert_eq!(done_b.result, Some(WireResult::TimeLimitDraw));
    assert_eq!(done_a.hash, done_b.hash);
    assert!(done_a.events.iter().any(|event| {
        event.tick == 11 && event.event == WireEvent::Finished(WireResult::TimeLimitDraw)
    }));
    assert!(done_b.events.iter().any(|event| {
        event.tick == 11 && event.event == WireEvent::Finished(WireResult::TimeLimitDraw)
    }));

    // After the linger window, both connections close with MatchOver.
    assert_eq!(first.error().await, ErrorCode::MatchOver);
    assert_eq!(second.error().await, ErrorCode::MatchOver);
    assert!(running.drained(WAIT).await);

    // Metrics record the time-limit draw and tickets cannot restart the match.
    let (_, metrics) = http(addr, "GET", "/metrics").await;
    assert!(
        metrics.contains("arena_matches_finished_total{result=\"time_limit_draw\"} 1\n"),
        "{metrics}"
    );
    let mut late = Client::open(addr).await;
    late.hello(from_hex(&created.tickets[0]).unwrap()).await;
    assert_eq!(late.error().await, ErrorCode::MatchOver);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn server_judges_timed_casts_resources_cooldowns_and_interrupts_over_websockets() {
    use arena_protocol::{WireAction, WireInterruptReason, WireSpell};
    use arena_sim::spells::{MEND_CAST_TICKS, RIFT_BOLT_CAST_TICKS, RIFT_BOLT_COST};

    // Ticks run at 200 Hz (5 ms) in this test, so allow proportional message rate while walking into range.
    let mut cfg = config(true);
    cfg.messages_per_second = 500;
    let running = start(cfg).await.unwrap();
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut second, _) = Client::join(addr, &created.tickets[1]).await;

    // Wait for running tick, then cast Mend on self (always in range/LoS from spawn).
    let start = first.snapshot_until(|s| s.tick > 0).await;
    assert_eq!(start.fighters[0].resource, 100);
    first
        .input(InputFrame {
            action: WireAction::Cast {
                spell: WireSpell::Mend,
                target: Some(0),
            },
            ..InputFrame::default()
        })
        .await;
    let started = first
        .snapshot_until(|s| {
            s.events.iter().any(|e| {
                matches!(
                    e.event,
                    WireEvent::CastStarted {
                        actor: 0,
                        spell: WireSpell::Mend,
                        target: Some(0),
                        duration_ticks: MEND_CAST_TICKS,
                    }
                )
            })
        })
        .await;
    // Timed casts charge on completion, so starting Mend spends nothing.
    assert_eq!(started.fighters[0].resource, 100);
    assert!(started.fighters[0].gcd_remaining > 0);
    assert!(started.fighters[0].active_cast.is_some());

    // Moving cancels the stationary Mend cast with Moved, and it spends nothing.
    first.input(forward()).await;
    let cancelled = first
        .snapshot_until(|s| {
            s.events.iter().any(|e| {
                e.event
                    == WireEvent::CastInterrupted {
                        actor: 0,
                        spell: WireSpell::Mend,
                        reason: WireInterruptReason::Moved,
                        by: None,
                    }
            })
        })
        .await;
    assert_eq!(cancelled.fighters[0].active_cast, None);
    assert_eq!(cancelled.fighters[0].cooldowns[3], 0);
    assert_eq!(cancelled.fighters[0].resource, 100);

    // Both players walk toward center until within Quell range (8 m).
    let mut latest = cancelled;
    while (i64::from(latest.fighters[0].z) - i64::from(latest.fighters[1].z)).abs() > 7_000 {
        first.input(forward()).await;
        second
            .input(InputFrame {
                movement: [0, 1000],
                ..InputFrame::default()
            })
            .await;
        let seq = first.seq;
        latest = first.snapshot_until(|s| s.ack == seq).await;
    }

    // Wait until Seat 0's GCD is clear and resource >= RIFT_BOLT_COST, then cast Rift Bolt.
    let _ready = first
        .snapshot_until(|s| {
            s.fighters[0].gcd_remaining == 0 && s.fighters[0].resource >= RIFT_BOLT_COST
        })
        .await;
    first
        .input(InputFrame {
            action: WireAction::Cast {
                spell: WireSpell::RiftBolt,
                target: Some(1),
            },
            ..InputFrame::default()
        })
        .await;
    let bolt_started = second
        .snapshot_until(|s| {
            s.events.iter().any(|e| {
                matches!(
                    e.event,
                    WireEvent::CastStarted {
                        actor: 0,
                        spell: WireSpell::RiftBolt,
                        target: Some(1),
                        duration_ticks: RIFT_BOLT_CAST_TICKS,
                    }
                )
            })
        })
        .await;
    assert!(bolt_started.fighters[0].active_cast.is_some());

    // Seat 1 casts Quell and interrupts Seat 0's Rift Bolt, applying lockout.
    second
        .input(InputFrame {
            action: WireAction::Cast {
                spell: WireSpell::Quell,
                target: Some(0),
            },
            ..InputFrame::default()
        })
        .await;
    let interrupted = first
        .snapshot_until(|s| {
            s.events.iter().any(|e| {
                e.event
                    == WireEvent::CastInterrupted {
                        actor: 0,
                        spell: WireSpell::RiftBolt,
                        reason: WireInterruptReason::Interrupted,
                        by: Some(1),
                    }
            })
        })
        .await;
    assert_eq!(interrupted.fighters[0].active_cast, None);
    assert!(interrupted.fighters[0].lockout_remaining > 0);
    // Neither the cancelled Mend nor the interrupted Rift Bolt spent anything.
    assert_eq!(interrupted.fighters[0].resource, 100);
    assert_eq!(interrupted.fighters[1].health, 100);

    // Seat 0 tries to Pulse while locked out -> Silenced rejection.
    first
        .input(InputFrame {
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        })
        .await;
    let silenced = first
        .snapshot_until(|s| {
            s.events
                .iter()
                .any(|e| e.event == WireEvent::Rejected(WireRejection::Silenced))
        })
        .await;
    assert_eq!(silenced.fighters[1].health, 100);

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn websocket_control_ping_flood_is_rate_limited_before_and_after_hello() {
    let mut cfg = config(true);
    cfg.messages_per_second = 10;
    let running = start(cfg).await.unwrap();
    let addr = running.addr;
    let created = new_match(addr, 1).await;

    // Pre-Hello WebSocket control Ping flood cannot bypass RateWindow.
    let mut pre_hello = Client::open(addr).await;
    for byte in 0..30u8 {
        pre_hello.raw(Message::Ping(vec![byte].into())).await;
    }
    assert_eq!(pre_hello.error().await, ErrorCode::RateLimited);

    // Post-Hello WebSocket control Ping flood is also rate-limited.
    let (mut joined, _) = Client::join(addr, &created.tickets[0]).await;
    for byte in 0..30u8 {
        joined.raw(Message::Ping(vec![byte].into())).await;
    }
    assert_eq!(joined.error().await, ErrorCode::RateLimited);

    assert_eq!(running.app.metrics().errors(ErrorCode::RateLimited), 2);
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn malformed_frames_and_invalid_intents_mid_match_isolate_offender_without_disrupting_peer() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut offender, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut peer, _) = Client::join(addr, &created.tickets[1]).await;

    let start_snap = offender.snapshot_until(|s| s.tick > 0).await;
    let spawn_pos = (start_snap.fighters[0].x, start_snap.fighters[0].z);

    // Out-of-bounds movement axes and impossible targets are rejected only to the sender.
    offender
        .input(InputFrame {
            movement: [i16::MAX, i16::MIN],
            action: InputFrame::pulse(u8::MAX),
            ..InputFrame::default()
        })
        .await;
    let rejected = offender
        .snapshot_until(|s| {
            s.events
                .iter()
                .any(|e| e.event == WireEvent::Rejected(WireRejection::InvalidMovement))
        })
        .await;
    assert!(
        rejected
            .events
            .iter()
            .any(|e| e.event == WireEvent::Rejected(WireRejection::InvalidTarget)),
        "expected impossible target rejection alongside invalid movement: {:?}",
        rejected.events
    );
    assert_eq!((rejected.fighters[0].x, rejected.fighters[0].z), spawn_pos);

    // Peer must never observe the offender's rejections on that tick.
    let peer_at_reject = peer.snapshot_until(|s| s.tick >= rejected.tick).await;
    assert!(
        !peer_at_reject
            .events
            .iter()
            .any(|e| matches!(e.event, WireEvent::Rejected(_))),
        "peer should not receive offender rejections: {:?}",
        peer_at_reject.events
    );

    // Stale/regressed sequence numbers are dropped without moving seat 0.
    let seq_before = offender.seq;
    offender
        .send(&ClientMessage::Input(InputFrame {
            seq: seq_before,
            movement: [0, -1000],
            ..InputFrame::default()
        }))
        .await;
    offender
        .send(&ClientMessage::Input(InputFrame {
            seq: 0,
            movement: [0, -1000],
            ..InputFrame::default()
        }))
        .await;
    let after_stale = offender
        .snapshot_until(|s| s.tick >= rejected.tick + 4)
        .await;
    assert_eq!(
        (after_stale.fighters[0].x, after_stale.fighters[0].z),
        spawn_pos
    );
    assert!(
        arena_server::metrics::Metrics::get(&running.app.metrics().inputs_stale) >= 2,
        "stale/regressed inputs must be counted and dropped"
    );

    // A malformed binary payload mid-match closes only the offender with Malformed.
    offender
        .raw(Message::binary(vec![1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]))
        .await;
    assert_eq!(offender.error().await, ErrorCode::Malformed);

    // Peer continues playing cleanly and observes seat 0 disconnected and still at spawn.
    let gone = peer
        .snapshot_until(|s| !s.fighters[0].connected && s.fighters[1].connected)
        .await;
    assert_eq!(gone.result, None);
    assert_eq!((gone.fighters[0].x, gone.fighters[0].z), spawn_pos);
    let peer_z_before = gone.fighters[1].z;
    peer.input(InputFrame {
        movement: [0, 1000],
        ..InputFrame::default()
    })
    .await;
    let peer_seq = peer.seq;
    let moved = peer.snapshot_until(|s| s.ack == peer_seq).await;
    assert_ne!(moved.fighters[1].z, peer_z_before);

    // Offender can still reconnect with their valid ticket and resume from spawn.
    let (mut rejoined, welcome) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome.actor, 0);
    let back = rejoined.snapshot_until(|s| s.fighters[0].connected).await;
    assert!(back.fighters[1].connected);
    assert_eq!((back.fighters[0].x, back.fighters[0].z), spawn_pos);

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn rapid_double_reconnect_and_takeover_preserves_seat_and_rejects_old_socket_inputs() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut first, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut peer, _) = Client::join(addr, &created.tickets[1]).await;

    let start = first.snapshot_until(|s| s.tick > 0).await;
    let start_z = start.fighters[0].z;

    // Rapid back-to-back takeovers of seat 0 while old sockets still try to send backward movement.
    let (mut second, welcome_two) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome_two.actor, 0);
    first
        .input(InputFrame {
            movement: [0, 1000],
            ..InputFrame::default()
        })
        .await;
    assert_eq!(first.error().await, ErrorCode::Replaced);

    let (mut third, welcome_three) = Client::join(addr, &created.tickets[0]).await;
    assert_eq!(welcome_three.actor, 0);
    second
        .input(InputFrame {
            movement: [0, 1000],
            ..InputFrame::default()
        })
        .await;
    assert_eq!(second.error().await, ErrorCode::Replaced);

    // Dropping/closing both replaced sockets must not detach the active third connection.
    first.leave().await;
    second.leave().await;

    // Wait a few ticks: replaced sockets' backward inputs must never move seat 0 or advance ack.
    let settled = third
        .snapshot_until(|s| s.tick >= start.tick + 4 && s.fighters[0].connected)
        .await;
    assert_eq!(settled.ack, 0);
    assert_eq!(settled.fighters[0].z, start_z);

    // Seat 0 remains connected and controlled exclusively by `third`.
    third.input(forward()).await;
    let after = third.snapshot_until(|s| s.ack == 1).await;
    assert!(after.fighters[0].z < start_z);
    assert!(after.fighters[0].connected);

    let peer_view = peer.snapshot_until(|s| s.tick >= after.tick).await;
    assert!(peer_view.fighters[0].connected);
    assert_eq!(peer_view.fighters[0].z, after.fighters[0].z);

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn live_ws_view_tick_round_trips_and_enforces_bounded_lag_comp() {
    let running = server().await;
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut a, welcome_a) = Client::join(addr, &created.tickets[0]).await;
    let (mut b, _) = Client::join(addr, &created.tickets[1]).await;
    assert_eq!(welcome_a.protocol, PROTOCOL_VERSION);
    assert_eq!(welcome_a.rules, arena_protocol::RULES_VERSION);

    // Wait for running snapshots so `view_tick` is non-zero.
    let s = a.snapshot_until(|s| s.tick >= 3).await;
    let _ = b.snapshot_until(|s| s.tick >= 3).await;

    // Submitting a future `view_tick` (u64::MAX) is clamped safely without errors or disconnects,
    // and out-of-range spawns still reject `OutOfRange`.
    a.input(InputFrame {
        action: InputFrame::pulse(1),
        view_tick: u64::MAX,
        ..InputFrame::default()
    })
    .await;
    let acked_seq = a.seq;
    let snap = a.snapshot_until(|snap| snap.ack == acked_seq).await;
    assert!(snap.tick > s.tick);
    assert!(snap.events.iter().any(|e| e.event
        == arena_protocol::WireEvent::Rejected(arena_protocol::WireRejection::OutOfRange)));

    // Subsequent normal input with realistic `view_tick` continues cleanly.
    a.input(InputFrame {
        movement: [0, -1000],
        view_tick: snap.tick.saturating_sub(2),
        ..InputFrame::default()
    })
    .await;
    let moved_seq = a.seq;
    let moved = a.snapshot_until(|snap| snap.ack == moved_seq).await;
    assert!(moved.fighters[0].z < snap.fighters[0].z);

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn live_ws_finished_match_delivers_identical_authoritative_scoreboard_matching_events() {
    use arena_protocol::{WireAction, WireSpell};

    let mut cfg = config(true);
    cfg.limits.linger = 4;
    // Test ticks run at 5 ms (200 Hz); raise the real-clock ingress cap proportionally.
    cfg.messages_per_second = 500;
    let running = start(cfg).await.unwrap();
    let addr = running.addr;
    let created = new_match(addr, 1).await;
    let (mut a, _) = Client::join(addr, &created.tickets[0]).await;
    let (mut b, _) = Client::join(addr, &created.tickets[1]).await;

    // Both seats walk toward center until within Pulse range (6 m).
    let mut snap_a = a.snapshot_until(|s| s.tick >= 1).await;
    while (i64::from(snap_a.fighters[0].z) - i64::from(snap_a.fighters[1].z)).abs() > 5_000 {
        a.input(forward()).await;
        b.input(InputFrame {
            movement: [0, 1000],
            ..InputFrame::default()
        })
        .await;
        let seq = a.seq;
        snap_a = a.snapshot_until(|s| s.ack == seq).await;
        assert_eq!(
            snap_a.scoreboard, None,
            "running snapshots have no scoreboard"
        );
    }

    // Tally every Hit/Healed event observed by seat 0 on the wire from here to finish.
    let mut observed_done = [0u32; 2];
    let mut observed_taken = [0u32; 2];
    let mut observed_heal_done = [0u32; 2];
    let mut observed_heal_recv = [0u32; 2];

    // Seat 0 hits seat 1 once (25 dmg), then seat 1 hits seat 0 once (25 dmg) and casts Mend
    // on itself (restoring 25 effective HP, capping 5 overheal), then seat 0 finishes seat 1.
    a.input(InputFrame {
        action: InputFrame::pulse(1),
        ..InputFrame::default()
    })
    .await;
    b.input(InputFrame {
        action: InputFrame::pulse(0),
        ..InputFrame::default()
    })
    .await;
    // Wait until both opening hits land and seat 1's GCD clears, then seat 1 casts Mend.
    let mut mend_started = false;
    let mut mend_healed = false;
    let mut final_a = None;
    let mut last_attack_seq = a.seq;
    while final_a.is_none() {
        let Some(msg) = a.next().await else {
            panic!("seat 0 closed before match finished");
        };
        let ServerMessage::Snapshot(snap) = msg else {
            continue;
        };
        for item in &snap.events {
            match item.event {
                WireEvent::Hit {
                    actor,
                    target,
                    damage,
                } => {
                    observed_done[usize::from(actor)] += u32::from(damage);
                    observed_taken[usize::from(target)] += u32::from(damage);
                }
                WireEvent::Healed {
                    actor,
                    target,
                    amount,
                } => {
                    observed_heal_done[usize::from(actor)] += u32::from(amount);
                    observed_heal_recv[usize::from(target)] += u32::from(amount);
                }
                _ => {}
            }
        }
        if snap.result.is_some() {
            final_a = Some(snap);
            break;
        }
        assert_eq!(snap.scoreboard, None);
        if !mend_started
            && snap.fighters[0].health < 100
            && snap.fighters[1].health < 100
            && snap.fighters[1].gcd_remaining == 0
        {
            mend_started = true;
            b.input(InputFrame {
                action: WireAction::Cast {
                    spell: WireSpell::Mend,
                    target: Some(1),
                },
                ..InputFrame::default()
            })
            .await;
            continue;
        }
        if mend_started && !mend_healed {
            if observed_heal_done[1] > 0 {
                mend_healed = true;
            } else {
                continue;
            }
        }
        if mend_healed
            && snap.ack >= last_attack_seq
            && snap.fighters[0].cooldowns[0] == 0
            && snap.fighters[0].gcd_remaining == 0
        {
            a.input(InputFrame {
                action: InputFrame::pulse(1),
                ..InputFrame::default()
            })
            .await;
            last_attack_seq = a.seq;
        }
    }

    let final_a = final_a.unwrap();
    let final_b = b.snapshot_until(|s| s.result.is_some()).await;
    assert_eq!(final_a.result, Some(WireResult::Won { team: 0 }));
    assert_eq!(final_b.result, final_a.result);
    assert_eq!(
        final_a.scoreboard, final_b.scoreboard,
        "both players receive identical authoritative scoreboards"
    );
    let board = final_a
        .scoreboard
        .as_ref()
        .expect("final snapshot carries scoreboard");
    assert_eq!(board.len(), 2);
    assert_eq!(board[0].damage_done, 125);
    assert_eq!(board[0].damage_taken, 25);
    assert_eq!(board[0].killing_blows, 1);
    assert_eq!(board[1].damage_done, 25);
    assert_eq!(board[1].damage_taken, 125);
    assert_eq!(board[1].healing_done, 25);
    assert_eq!(board[1].healing_received, 25);
    assert_eq!(board[1].killing_blows, 0);
    for seat in 0..2 {
        assert_eq!(board[seat].damage_done, observed_done[seat]);
        assert_eq!(board[seat].damage_taken, observed_taken[seat]);
        assert_eq!(board[seat].healing_done, observed_heal_done[seat]);
        assert_eq!(board[seat].healing_received, observed_heal_recv[seat]);
    }
    // After the linger window, both connections close cleanly with MatchOver.
    assert_eq!(a.error().await, ErrorCode::MatchOver);
    assert_eq!(b.error().await, ErrorCode::MatchOver);

    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn server_validates_locks_and_relays_player_appearances_across_reconnects() {
    use arena_protocol::WireAppearance;

    let running = server().await;
    let created = new_match(running.addr, 1).await;

    let look_a = WireAppearance {
        ancestry: "human".into(),
        body: "female".into(),
        skin_tone: "natural".into(),
        hair_color: "auburn".into(),
        facial_hair_color: "auburn".into(),
        eyes: "brown".into(),
        build: 0,
        groups: vec![
            ("hair".into(), "Hair_Buns".into()),
            ("torso".into(), "Ranger".into()),
        ],
        variants: vec![("Ranger".into(), "green".into())],
    };
    let look_b = WireAppearance {
        ancestry: "elf".into(),
        body: "male".into(),
        skin_tone: "fair".into(),
        hair_color: "platinum".into(),
        facial_hair_color: "platinum".into(),
        eyes: "blue".into(),
        build: 50,
        groups: vec![("hair".into(), "Race_Hair".into())],
        variants: Vec::new(),
    };
    let swapped = WireAppearance {
        ancestry: "orc".into(),
        body: "male".into(),
        skin_tone: "olive".into(),
        hair_color: "black".into(),
        facial_hair_color: "black".into(),
        eyes: "amber".into(),
        build: 100,
        groups: Vec::new(),
        variants: Vec::new(),
    };

    // Malformed look with path traversal is rejected with Malformed before joining.
    let mut bad_client = Client::open(running.addr).await;
    let mut bad_look = look_a.clone();
    bad_look.body = "../secret".into();
    bad_client
        .send(&ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket: from_hex(&created.tickets[0]).unwrap(),
            appearance: Some(Box::new(bad_look)),
        })
        .await;
    assert_eq!(bad_client.error().await, ErrorCode::Malformed);

    // Seat 0 joins with look_a and receives its own locked look.
    let mut a = Client::open(running.addr).await;
    a.send(&ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: from_hex(&created.tickets[0]).unwrap(),
        appearance: Some(Box::new(look_a.clone())),
    })
    .await;
    assert!(matches!(a.next().await, Some(ServerMessage::Welcome(_))));
    assert!(matches!(a.next().await, Some(ServerMessage::Snapshot(_))));
    assert_eq!(
        a.next().await,
        Some(ServerMessage::Appearances(vec![Some(look_a.clone()), None]))
    );

    // Seat 1 joins with look_b; both seat 0 and seat 1 receive both locked looks.
    let mut b = Client::open(running.addr).await;
    b.send(&ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: from_hex(&created.tickets[1]).unwrap(),
        appearance: Some(Box::new(look_b.clone())),
    })
    .await;
    assert!(matches!(b.next().await, Some(ServerMessage::Welcome(_))));
    assert!(matches!(b.next().await, Some(ServerMessage::Snapshot(_))));
    assert_eq!(
        b.next().await,
        Some(ServerMessage::Appearances(vec![
            Some(look_a.clone()),
            Some(look_b.clone()),
        ]))
    );
    let mut a_saw_both = false;
    for _ in 0..16 {
        if let Some(ServerMessage::Appearances(looks)) = a.next().await {
            assert_eq!(looks, vec![Some(look_a.clone()), Some(look_b.clone())]);
            a_saw_both = true;
            break;
        }
    }
    assert!(a_saw_both, "seat 0 received seat 1's locked look");

    // Seat 0 reconnects trying to swap to `swapped`; the server keeps the original locked `look_a`.
    let mut a2 = Client::open(running.addr).await;
    a2.send(&ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: from_hex(&created.tickets[0]).unwrap(),
        appearance: Some(Box::new(swapped)),
    })
    .await;
    assert!(matches!(a2.next().await, Some(ServerMessage::Welcome(_))));
    assert!(matches!(a2.next().await, Some(ServerMessage::Snapshot(_))));
    assert_eq!(
        a2.next().await,
        Some(ServerMessage::Appearances(vec![
            Some(look_a.clone()),
            Some(look_b.clone()),
        ]))
    );
    assert_eq!(a.error().await, ErrorCode::Replaced);

    a2.leave().await;
    b.leave().await;
    running.shut_down(Duration::ZERO).await.unwrap();
}

#[tokio::test]
async fn live_ws_spectators_cannot_act_trail_by_delay_respect_cap_and_receive_no_private_events() {
    let mut cfg = config(true);
    cfg.limits.spectator_delay = 10;
    cfg.limits.max_spectators = 2;
    let running = start(cfg).await.unwrap();
    let created = running.app.issue_match_with_spectators(1, 3).unwrap();

    // Two players join and start the match.
    let (mut a, welcome_a) = Client::join(running.addr, &created.tickets[0]).await;
    let (b, welcome_b) = Client::join(running.addr, &created.tickets[1]).await;
    assert!(!welcome_a.spectator && welcome_a.actor == 0);
    assert!(!welcome_b.spectator && welcome_b.actor == 1);

    // Two spectators join within the cap (max_spectators = 2).
    let (mut spec1, w1) = Client::join(running.addr, &created.spectator_tickets[0]).await;
    let (spec2, w2) = Client::join(running.addr, &created.spectator_tickets[1]).await;
    assert!(w1.spectator && w1.actor == 0);
    assert!(w2.spectator && w2.actor == 0);

    // A third distinct spectator exceeds the cap -> MatchFull.
    let mut spec3 = Client::open(running.addr).await;
    spec3
        .hello(from_hex(&created.spectator_tickets[2]).unwrap())
        .await;
    assert_eq!(spec3.error().await, ErrorCode::MatchFull);

    // Wait until the match has ticked into Running so `a`'s OutOfRange Pulse is
    // evaluated in live combat rather than discarded during the Waiting transition.
    let mut seat0_spawn_x = loop {
        if let Some(ServerMessage::Snapshot(snap)) = a.next().await
            && snap.tick >= 1
        {
            break snap.fighters.first().map(|f| f.x);
        }
    };

    // Spectator inputs are strictly ignored by the server (never move or attack any fighter),
    // while Ping still gets Pong.
    spec1
        .send(&ClientMessage::Input(InputFrame {
            seq: 1,
            movement: [1000, 0],
            action: InputFrame::pulse(1),
            ..InputFrame::default()
        }))
        .await;
    spec1.send(&ClientMessage::Ping { nonce: 77 }).await;
    // Seat 0 also triggers an OutOfRange rejection so we verify spectators never see Rejected.
    a.send(&ClientMessage::Input(InputFrame {
        seq: 1,
        action: InputFrame::pulse(1),
        ..InputFrame::default()
    }))
    .await;

    let mut saw_pong = false;
    let mut saw_player_rejection = false;
    let mut latest_player_tick = 0u64;
    let mut latest_spec_tick = 0u64;
    let mut seat0_moved = false;
    for _ in 0..40 {
        if let Some(ServerMessage::Snapshot(snap)) = a.next().await {
            latest_player_tick = latest_player_tick.max(snap.tick);
            if let Some(f0) = snap.fighters.first() {
                let spawn_x = *seat0_spawn_x.get_or_insert(f0.x);
                if f0.x != spawn_x {
                    seat0_moved = true;
                }
            }
            if snap
                .events
                .iter()
                .any(|e| matches!(e.event, arena_protocol::WireEvent::Rejected(_)))
            {
                saw_player_rejection = true;
            }
            if latest_player_tick >= 16 && saw_player_rejection {
                break;
            }
        }
    }
    assert!(saw_player_rejection, "player seat saw its own rejection");
    assert!(
        !seat0_moved,
        "spectator movement input must never move seat 0"
    );
    assert!(latest_player_tick >= 16);

    // Drain spectator messages: must receive Pong, delayed snapshots (never ahead of live - delay),
    // and ZERO WireEvent::Rejected frames!
    let mut spec_frames = 0usize;
    while spec_frames < 30 {
        match tokio::time::timeout(Duration::from_millis(200), spec1.next()).await {
            Ok(Some(ServerMessage::Pong { nonce: 77 })) => saw_pong = true,
            Ok(Some(ServerMessage::Snapshot(snap))) => {
                assert!(
                    !snap
                        .events
                        .iter()
                        .any(|e| matches!(e.event, arena_protocol::WireEvent::Rejected(_))),
                    "spectator must never receive private Rejected events"
                );
                assert_eq!(snap.ack, 0);
                assert_eq!(snap.queued, 0);
                latest_spec_tick = latest_spec_tick.max(snap.tick);
                spec_frames += 1;
            }
            _ => break,
        }
    }
    assert!(saw_pong, "spectator pings still receive pongs");
    assert!(
        latest_spec_tick > 0,
        "spectator received delayed snapshots once the delay elapsed"
    );

    spec1.leave().await;
    spec2.leave().await;
    a.leave().await;
    b.leave().await;
    running.shut_down(Duration::ZERO).await.unwrap();
}
