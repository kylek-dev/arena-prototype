//! Hub worlds over real WebSockets, and the instance directory (ADR 0057
//! slice 3). Ticks are shortened to 5 ms; the rules still count 30 a second.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{ClientMessage, ErrorCode, PROTOCOL_VERSION, ServerMessage, Snapshot};
use arena_world::ticket::{Holder, TicketKey, issue_hub};
use arena_world::worlds::{DIRECTORY_LABEL, DIRECTORY_SIGNATURE, DirectoryReply, DirectoryRequest};
use arena_world::{Config, Running, start};
use futures_util::stream::SplitSink;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const WAIT: Duration = Duration::from_secs(5);

async fn server() -> Running {
    let mut config = Config::new(TicketKey::random());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.tick = Duration::from_millis(5);
    config.connections_per_address = 0;
    start(config).await.unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

fn ticket(server: &Running, instance: u64, player: u64) -> Vec<u8> {
    issue_hub(
        server.app.key(),
        instance,
        player,
        Holder {
            account: None,
            handle: arena_protocol::Handle::new("Rook", player as u16),
            admin: false,
        },
        now() + 600,
    )
}

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

struct Client {
    sink: SplitSink<Socket, Message>,
    inbox: mpsc::UnboundedReceiver<ServerMessage>,
}
impl Client {
    async fn join(addr: SocketAddr, ticket: Vec<u8>) -> Self {
        let (socket, _) = connect_async(format!("ws://{addr}/play")).await.unwrap();
        let (mut sink, mut stream) = socket.split();
        let (sender, inbox) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            while let Some(Ok(message)) = stream.next().await {
                if let Message::Binary(frame) = message {
                    let message = ServerMessage::decode(&frame).expect("server frames decode");
                    if sender.send(message).is_err() {
                        break;
                    }
                }
            }
        });
        let hello = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket,
            appearance: None,
        };
        sink.send(Message::binary(hello.encode())).await.unwrap();
        Self { sink, inbox }
    }
    async fn next(&mut self) -> Option<ServerMessage> {
        tokio::time::timeout(WAIT, self.inbox.recv())
            .await
            .expect("server answered in time")
    }
    async fn until<T>(&mut self, wanted: impl Fn(&ServerMessage) -> Option<T>) -> T {
        loop {
            let message = self.next().await.expect("still open");
            if let Some(found) = wanted(&message) {
                return found;
            }
        }
    }
    async fn snapshot(&mut self) -> Snapshot {
        self.until(|message| match message {
            ServerMessage::Snapshot(snapshot) => Some(snapshot.clone()),
            _ => None,
        })
        .await
    }
    async fn leave(mut self) {
        let _ = self.sink.close().await;
    }
}

async fn report(
    addr: SocketAddr,
    key: &TicketKey,
    request: &DirectoryRequest,
    tamper: bool,
) -> reqwest::Response {
    let body = serde_json::to_vec(request).unwrap();
    let mut tag = key.sign(DIRECTORY_LABEL, &body);
    if tamper {
        tag = key.sign("other", &body);
    }
    let _ = rustls::crypto::ring::default_provider().install_default();
    reqwest::Client::new()
        .post(format!("http://{addr}/worlds/report"))
        .header(DIRECTORY_SIGNATURE, tag)
        .body(body)
        .send()
        .await
        .unwrap()
}

#[tokio::test]
async fn hub_tickets_join_one_world_where_players_see_each_other() {
    let server = server().await;
    let mut first = Client::join(server.addr, ticket(&server, 7, 1)).await;
    let welcome = first
        .until(|message| match message {
            ServerMessage::Welcome(welcome) => Some(*welcome),
            _ => None,
        })
        .await;
    assert!(welcome.hub);
    let mut second = Client::join(server.addr, ticket(&server, 7, 2)).await;
    // The first hears who arrived (after their own entry), then sees them.
    let newcomer = arena_protocol::Handle::new("Rook", 2);
    first
        .until(|message| match message {
            ServerMessage::Member(member) if member.handle == newcomer => Some(()),
            _ => None,
        })
        .await;
    let snapshot = first.snapshot().await;
    let world = snapshot.hub_arena().expect("a valid world");
    assert_eq!(world.players(), 2);
    assert_eq!(snapshot.hash, world.state_hash());
    let _ = second.snapshot().await;

    // A different instance is a different world.
    let mut other = Client::join(server.addr, ticket(&server, 8, 3)).await;
    let alone = other.snapshot().await;
    assert_eq!(alone.hub_arena().unwrap().players(), 1);
    assert_eq!(server.app.world_reports().len(), 2);

    second.leave().await;
    other.leave().await;
    first.leave().await;
}

#[tokio::test]
async fn the_directory_reports_worlds_and_applies_blocks_only_when_signed_and_fresh() {
    let server = server().await;
    let key = server.app.key();
    let mut a = Client::join(server.addr, ticket(&server, 9, 11)).await;
    let mut b = Client::join(server.addr, ticket(&server, 9, 12)).await;
    let _ = a.snapshot().await;
    let _ = b.snapshot().await;
    // Wait for a tick to publish the report with both.
    let start = std::time::Instant::now();
    let reply = loop {
        let request = DirectoryRequest {
            now_ms: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            blocks: Vec::new(),
            players: None,
        };
        let reply = report(server.addr, key, &request, false).await;
        assert_eq!(reply.status(), 200);
        let reply: DirectoryReply = reply.json().await.unwrap();
        if reply.worlds.first().is_some_and(|w| w.players.len() == 2) {
            break reply;
        }
        assert!(start.elapsed() < WAIT, "{reply:?}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(reply.worlds[0].instance, format!("{:016x}", 9));
    let mut players = reply.worlds[0].players.clone();
    players.sort_unstable();
    assert_eq!(players, vec![11, 12]);
    assert_eq!(reply.worlds[0].capacity, arena_sim::arena::HUB_PLAYER_CAP);

    let fresh = |offset: u64| DirectoryRequest {
        now_ms: (now() + 1) * 1000 + offset,
        blocks: vec![(11, 12)],
        players: None,
    };
    // Unsigned (or signed for something else): refused, nothing changes.
    assert_eq!(
        report(server.addr, key, &fresh(1), true).await.status(),
        401
    );
    // Too far from the server's clock.
    let stale = DirectoryRequest {
        now_ms: 1_000,
        blocks: vec![(11, 12)],
        players: None,
    };
    assert_eq!(report(server.addr, key, &stale, false).await.status(), 401);
    // Accepted, and the same request again is a replay.
    let blocking = fresh(2);
    assert_eq!(
        report(server.addr, key, &blocking, false).await.status(),
        200
    );
    assert_eq!(
        report(server.addr, key, &blocking, false).await.status(),
        409
    );
    // The block reaches the world: each hears the other left their view.
    a.until(|message| matches!(message, ServerMessage::MemberLeft { .. }).then_some(()))
        .await;
    b.until(|message| matches!(message, ServerMessage::MemberLeft { .. }).then_some(()))
        .await;
    let seen = a.snapshot().await;
    assert_eq!(seen.hub_arena().unwrap().players(), 1, "B is hidden from A");

    a.leave().await;
    b.leave().await;
}

#[tokio::test]
async fn a_full_world_says_so() {
    let server = server().await;
    let mut clients = Vec::new();
    for player in 0..arena_sim::arena::HUB_PLAYER_CAP as u64 {
        let mut client = Client::join(server.addr, ticket(&server, 5, player)).await;
        let _ = client.snapshot().await;
        clients.push(client);
    }
    let mut extra = Client::join(server.addr, ticket(&server, 5, 99)).await;
    let code = extra
        .until(|message| match message {
            ServerMessage::Error(code) => Some(*code),
            _ => None,
        })
        .await;
    assert_eq!(code, ErrorCode::MatchFull);
    for client in clients {
        client.leave().await;
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

async fn raw_report(addr: SocketAddr, tag: Option<String>, body: Vec<u8>) -> u16 {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut request = reqwest::Client::new()
        .post(format!("http://{addr}/worlds/report"))
        .body(body);
    if let Some(tag) = tag {
        request = request.header(DIRECTORY_SIGNATURE, tag);
    }
    request.send().await.unwrap().status().as_u16()
}

#[tokio::test]
async fn bad_directory_requests_are_refused() {
    let server = server().await;
    let key = server.app.key();
    let body = br#"{"now_ms": 1, "blocks": "#.to_vec();
    // No signature, a signed body that is not JSON, and an oversized body.
    assert_eq!(raw_report(server.addr, None, b"{}".to_vec()).await, 401);
    let tag = key.sign(DIRECTORY_LABEL, &body);
    assert_eq!(raw_report(server.addr, Some(tag), body).await, 400);
    let huge = vec![b' '; arena_world::worlds::MAX_DIRECTORY_BODY + 1];
    let tag = key.sign(DIRECTORY_LABEL, &huge);
    assert_eq!(raw_report(server.addr, Some(tag), huge).await, 413);
}

#[tokio::test]
async fn blocks_reported_before_a_world_exists_apply_from_its_first_arrival() {
    let server = server().await;
    let key = server.app.key();
    let request = DirectoryRequest {
        now_ms: now_ms(),
        blocks: vec![(21, 22)],
        players: None,
    };
    assert_eq!(
        report(server.addr, key, &request, false).await.status(),
        200
    );
    let mut a = Client::join(server.addr, ticket(&server, 40, 21)).await;
    let _ = a.snapshot().await;
    let mut b = Client::join(server.addr, ticket(&server, 40, 22)).await;
    let seen = b.snapshot().await;
    assert_eq!(seen.hub_arena().unwrap().players(), 1, "A is hidden from B");
    // A never hears of B: a few snapshots later, still no member entry.
    for _ in 0..5 {
        let message = a.next().await.unwrap();
        assert!(
            !matches!(&message, ServerMessage::Member(m) if m.handle == arena_protocol::Handle::new("Rook", 22)),
            "{message:?}"
        );
    }
    a.leave().await;
    b.leave().await;
}

#[tokio::test]
async fn an_empty_world_ends_and_the_same_id_starts_a_fresh_one() {
    let mut config = Config::new(TicketKey::random());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.tick = Duration::from_millis(2);
    config.connections_per_address = 0;
    config.world_limits = arena_world::world::WorldLimits {
        queue: 32,
        grace: 2,
        idle: 5,
        keyframe_every: 30,
    };
    let server = start(config).await.unwrap();
    let mut first = Client::join(server.addr, ticket(&server, 50, 1)).await;
    let _ = first.snapshot().await;
    let metrics = server.app.metrics();
    assert_eq!(
        metrics
            .worlds_active
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    first.leave().await;
    let start = std::time::Instant::now();
    while !server.app.world_reports().is_empty() {
        assert!(start.elapsed() < WAIT, "the world ends");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let gauge =
        |value: &std::sync::atomic::AtomicU64| value.load(std::sync::atomic::Ordering::Relaxed);
    let start = std::time::Instant::now();
    while gauge(&metrics.worlds_active) != 0 || gauge(&metrics.world_players) != 0 {
        assert!(start.elapsed() < WAIT, "gauges return to zero");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let mut again = Client::join(server.addr, ticket(&server, 50, 1)).await;
    let snapshot = again.snapshot().await;
    assert_eq!(snapshot.hub_arena().unwrap().players(), 1);
    assert_eq!(gauge(&metrics.worlds_active), 1);
    again.leave().await;
}
