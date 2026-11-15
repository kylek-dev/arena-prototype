//! End-to-end tests over real WebSockets against a server on loopback.
//! Ticks are shortened to 5 ms; the rules still count 30 ticks per second.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{
    ClientMessage, ErrorCode, FighterState, InputFrame, PROTOCOL_VERSION, ServerMessage, Snapshot,
    Welcome, WireEvent, WireRejection,
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
            attack: Some(0),
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
                seat: 0,
                expires: 1,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "seat outside the match",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id,
                player: 1,
                seat: 9,
                expires: far_future,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "match that does not exist",
            Message::binary(hello(signed(Claims {
                match_id: created.match_id ^ 1,
                player: 1,
                seat: 0,
                expires: far_future,
            }))),
            ErrorCode::MatchOver,
        ),
        (
            "another ticket key",
            Message::binary(hello(TicketKey::random().issue(&Claims {
                match_id: created.match_id,
                player: 1,
                seat: 0,
                expires: far_future,
            }))),
            ErrorCode::BadTicket,
        ),
        (
            "future protocol",
            Message::binary(
                ClientMessage::Hello {
                    protocol: PROTOCOL_VERSION + 1,
                    ticket: valid.clone(),
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
    assert_eq!(metrics.errors(ErrorCode::BadTicket), 4);
    assert_eq!(metrics.errors(ErrorCode::Malformed), 4);
    assert_eq!(metrics.errors(ErrorCode::HelloTimeout), 1);
    running.shut_down(Duration::ZERO).await.unwrap();
}

fn hello(ticket: Vec<u8>) -> Vec<u8> {
    ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket,
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
    assert!(old.closed().await, "the replaced socket is closed");
    let start = new.snapshot_until(|_| true).await.fighters[0];
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
    new_match(addr, 3).await;
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
