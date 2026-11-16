//! The front door against a production-mode match server sharing its key:
//! links it hands out start and join a match; origins, request shape and
//! the per-address creation limit are enforced.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use arena_server::door::{self, CreatedMatch, DoorConfig};
use arena_server::ingress::Origins;
use arena_server::ticket::{TicketKey, from_hex};
use arena_server::{Config, start};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const PAGE: &str = "https://arena.example.com/";
const WAIT: Duration = Duration::from_secs(5);

struct Reply {
    status: u16,
    head: String,
    body: String,
}

async fn request(addr: SocketAddr, method: &str, path: &str, headers: &str, body: &str) -> Reply {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "{method} {path} HTTP/1.1\r\nHost: test\r\n{headers}Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    Reply {
        status: response[9..12].parse().unwrap(),
        head: head.to_ascii_lowercase(),
        body: body.to_owned(),
    }
}

const JSON: &str = "Content-Type: application/json\r\n";

async fn create(addr: SocketAddr, extra: &str, team_size: usize) -> Reply {
    request(
        addr,
        "POST",
        "/api/matches",
        &format!("{JSON}{extra}"),
        &format!("{{\"team_size\":{team_size}}}"),
    )
    .await
}

/// The `server` and `ticket` a join link's fragment carries, decoded the
/// way the browser's `URLSearchParams` reads them.
fn fragment(link: &str) -> (String, String) {
    let (page, fragment) = link.split_once('#').unwrap();
    assert_eq!(page, PAGE);
    let mut server = None;
    let mut ticket = None;
    for pair in fragment.split('&') {
        let (name, value) = pair.split_once('=').unwrap();
        match name {
            "server" => server = Some(value.replace("%3A", ":").replace("%2F", "/")),
            "ticket" => ticket = Some(value.to_owned()),
            "expires" => assert!(value.parse::<u64>().is_ok()),
            other => panic!("unexpected {other}"),
        }
    }
    (server.unwrap(), ticket.unwrap())
}

#[tokio::test]
async fn links_from_the_front_door_start_and_join_a_match() {
    let key = TicketKey::random();
    let mut server_config = Config::new(key.clone());
    server_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    server_config.tick = Duration::from_millis(5);
    server_config.origins = Origins::Only(vec!["https://arena.example.com".into()]);
    let server = start(server_config).await.unwrap();

    let play = format!("ws://{}/play", server.addr);
    let mut config = DoorConfig::new(key, play.clone(), PAGE.into());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.origins = Origins::Only(vec!["https://arena.example.com".into()]);
    config.creation_burst = 2;
    config.creation_every = Duration::from_secs(60);
    let running = door::start(config).await.unwrap();
    let addr = running.addr;

    let page = "Origin: https://arena.example.com\r\n";
    let reply = create(addr, page, 1).await;
    assert_eq!(reply.status, 200, "{}", reply.body);
    assert!(
        reply.head.contains("cache-control: no-store"),
        "{}",
        reply.head
    );
    let created: CreatedMatch = serde_json::from_str(&reply.body).unwrap();
    assert_eq!(created.team_size, 1);
    let teams: Vec<_> = created
        .seats
        .iter()
        .map(|seat| seat.team.as_str())
        .collect();
    assert_eq!(teams, ["A", "B"]);

    // Both players open their links: the first starts the match.
    let mut players = Vec::new();
    for (seat, link) in created.seats.iter().enumerate() {
        let (server_url, ticket) = fragment(&link.link);
        assert_eq!(server_url, play);
        let (mut socket, _) = connect_async(&server_url).await.unwrap();
        let hello = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket: from_hex(&ticket).unwrap(),
        };
        socket.send(Message::binary(hello.encode())).await.unwrap();
        let welcome = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match ServerMessage::decode(&welcome.into_data()).unwrap() {
            ServerMessage::Welcome(welcome) => assert_eq!(usize::from(welcome.actor), seat),
            other => panic!("expected a welcome, got {other:?}"),
        }
        players.push(socket);
    }
    assert_eq!(server.active_matches(), 1);

    // Other websites, malformed requests and impossible sizes are refused.
    let elsewhere = create(addr, "Origin: https://elsewhere.example\r\n", 1).await;
    assert_eq!(elsewhere.status, 403);
    assert_eq!(create(addr, page, 4).await.status, 400);
    let not_json = request(addr, "POST", "/api/matches", page, "team_size=1").await;
    assert_eq!(not_json.status, 415);
    let oversized = request(
        addr,
        "POST",
        "/api/matches",
        &format!("{JSON}{page}"),
        &format!("{{\"team_size\":1,\"pad\":\"{}\"}}", "x".repeat(4096)),
    )
    .await;
    assert_eq!(oversized.status, 413);

    // One more creation fits the burst of two; then the address waits.
    assert_eq!(create(addr, page, 3).await.status, 200);
    let limited = create(addr, page, 1).await;
    assert_eq!(limited.status, 429);
    assert!(limited.head.contains("retry-after: 60"), "{}", limited.head);

    // The lobby page is served with a strict content policy.
    let lobby = request(addr, "GET", "/lobby/", "", "").await;
    assert_eq!(lobby.status, 200);
    assert!(
        lobby
            .head
            .contains("content-security-policy: default-src 'none'")
    );
    assert!(lobby.body.contains("Start a match"));
    assert_eq!(request(addr, "GET", "/lobby", "", "").await.status, 308);
    for asset in ["lobby.js", "lobby.css", "palette.css"] {
        let reply = request(addr, "GET", &format!("/lobby/{asset}"), "", "").await;
        assert_eq!(reply.status, 200, "{asset}");
    }
    let metrics = request(addr, "GET", "/metrics", "", "").await.body;
    assert!(
        metrics.contains("arena_door_matches_issued_total 2\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("arena_door_refused_rate_total 1\n"),
        "{metrics}"
    );
    assert!(
        metrics.contains("arena_door_refused_origin_total 1\n"),
        "{metrics}"
    );

    drop(players);
    running.stop().await.unwrap();
    server.stop().await.unwrap();
}

mod queue {
    use super::*;
    use arena_server::door::QueueMessage;
    use futures_util::stream::{SplitSink, SplitStream};
    use tokio_tungstenite::tungstenite::client::IntoClientRequest;
    use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

    type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

    struct Queuer {
        sink: SplitSink<Socket, Message>,
        stream: SplitStream<Socket>,
    }
    impl Queuer {
        async fn open(addr: SocketAddr) -> Self {
            let mut request = format!("ws://{addr}/api/queue")
                .into_client_request()
                .unwrap();
            request
                .headers_mut()
                .insert("origin", "https://arena.example.com".parse().unwrap());
            let (socket, _) = connect_async(request).await.unwrap();
            let (sink, stream) = socket.split();
            Self { sink, stream }
        }
        async fn say(&mut self, json: &str) {
            self.sink.send(Message::text(json)).await.unwrap();
        }
        async fn join(addr: SocketAddr, mode: &str, guest: u128) -> Self {
            let mut queuer = Self::open(addr).await;
            queuer
                .say(&format!(
                    r#"{{"type":"join","mode":"{mode}","guest":"{guest:032x}"}}"#
                ))
                .await;
            queuer
        }
        /// The next message, or `None` once the door closed the socket.
        async fn next(&mut self) -> Option<QueueMessage> {
            loop {
                match tokio::time::timeout(WAIT, self.stream.next())
                    .await
                    .unwrap()
                {
                    Some(Ok(Message::Text(text))) => {
                        return Some(serde_json::from_str(&text).unwrap());
                    }
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return None,
                    Some(Ok(_)) => {}
                }
            }
        }
        /// Skips status updates.
        async fn next_event(&mut self) -> Option<QueueMessage> {
            loop {
                match self.next().await {
                    Some(QueueMessage::Queued { .. }) => {}
                    other => return other,
                }
            }
        }
    }

    async fn door_and_server(ready: Duration) -> (door::DoorRunning, arena_server::Running) {
        let key = TicketKey::random();
        let mut server_config = Config::new(key.clone());
        server_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
        server_config.tick = Duration::from_millis(5);
        let server = start(server_config).await.unwrap();
        let mut config = DoorConfig::new(key, format!("ws://{}/play", server.addr), PAGE.into());
        config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
        config.origins = Origins::Only(vec!["https://arena.example.com".into()]);
        config.ready_timeout = ready;
        (door::start(config).await.unwrap(), server)
    }

    #[tokio::test]
    async fn four_solo_players_are_matched_and_each_joins_their_own_seat() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        let mut queuers = Vec::new();
        for guest in 1..=3u128 {
            let mut queuer = Queuer::join(door.addr, "solo-2v2", guest).await;
            match queuer.next().await {
                Some(QueueMessage::Queued { mode, waiting }) => {
                    assert_eq!((mode.as_str(), waiting), ("solo-2v2", guest as usize));
                }
                other => panic!("expected queued, got {other:?}"),
            }
            queuers.push(queuer);
        }
        queuers.push(Queuer::join(door.addr, "solo-2v2", 4).await);
        for queuer in &mut queuers {
            assert_eq!(
                queuer.next_event().await,
                Some(QueueMessage::Found {
                    mode: "solo-2v2".into()
                })
            );
            queuer.say(r#"{"type":"ready"}"#).await;
        }
        let mut actors = Vec::new();
        let mut seats = Vec::new();
        for queuer in &mut queuers {
            let Some(QueueMessage::Match {
                mode,
                server: play,
                ticket,
                ..
            }) = queuer.next_event().await
            else {
                panic!("expected a match");
            };
            assert_eq!(mode, "solo-2v2");
            assert_eq!(queuer.next().await, None, "the door closes after the seat");
            let (mut socket, _) = connect_async(&play).await.unwrap();
            let hello = ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                ticket: from_hex(&ticket).unwrap(),
            };
            socket.send(Message::binary(hello.encode())).await.unwrap();
            let frame = tokio::time::timeout(WAIT, socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match ServerMessage::decode(&frame.into_data()).unwrap() {
                ServerMessage::Welcome(welcome) => {
                    assert_eq!(welcome.team_size, 2);
                    actors.push(welcome.actor);
                }
                other => panic!("expected a welcome, got {other:?}"),
            }
            seats.push(socket);
        }
        actors.sort();
        assert_eq!(actors, [0, 1, 2, 3], "four distinct seats in one match");
        assert_eq!(server.active_matches(), 1);
        let metrics = request(door.addr, "GET", "/metrics", "", "").await.body;
        assert!(
            metrics.contains("arena_door_queue_matches_total 1\n"),
            "{metrics}"
        );
        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn no_shows_are_dropped_and_the_others_go_back_to_the_front() {
        let (door, server) = door_and_server(Duration::from_millis(300)).await;
        let mut first = Queuer::join(door.addr, "1v1", 1).await;
        first.next().await;
        let mut absent = Queuer::join(door.addr, "1v1", 2).await;
        assert!(matches!(
            first.next_event().await,
            Some(QueueMessage::Found { .. })
        ));
        assert!(matches!(
            absent.next_event().await,
            Some(QueueMessage::Found { .. })
        ));
        first.say(r#"{"type":"ready"}"#).await;
        // The absent player never confirms.
        match absent.next_event().await {
            Some(QueueMessage::Error { code, .. }) => assert_eq!(code, "not_ready"),
            other => panic!("expected not_ready, got {other:?}"),
        }
        assert_eq!(
            first.next_event().await,
            Some(QueueMessage::Requeued { mode: "1v1".into() })
        );
        // A newcomer is matched with the requeued player straight away.
        let mut newcomer = Queuer::join(door.addr, "1v1", 3).await;
        assert!(matches!(
            first.next_event().await,
            Some(QueueMessage::Found { .. })
        ));
        assert!(matches!(
            newcomer.next_event().await,
            Some(QueueMessage::Found { .. })
        ));
        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn the_queue_refuses_strangers_bad_messages_and_double_entries() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        // Another website's page cannot queue its visitors.
        let mut request = format!("ws://{}/api/queue", door.addr)
            .into_client_request()
            .unwrap();
        request
            .headers_mut()
            .insert("origin", "https://elsewhere.example".parse().unwrap());
        match connect_async(request).await {
            Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
                assert_eq!(response.status().as_u16(), 403)
            }
            other => panic!("expected 403, got {:?}", other.map(|(_, r)| r.status())),
        }
        for bad in [
            r#"{"type":"ready"}"#,
            r#"{"type":"join","mode":"2v2","guest":"00000000000000000000000000000001"}"#,
            r#"{"type":"join","mode":"1v1","guest":"me"}"#,
            "hello",
        ] {
            let mut queuer = Queuer::open(door.addr).await;
            queuer.say(bad).await;
            match queuer.next().await {
                Some(QueueMessage::Error { code, .. }) => assert_eq!(code, "bad_request", "{bad}"),
                other => panic!("{bad}: expected bad_request, got {other:?}"),
            }
        }
        // Queueing again from another tab replaces the first entry.
        let mut tab = Queuer::join(door.addr, "solo-3v3", 7).await;
        tab.next().await;
        let mut other_tab = Queuer::join(door.addr, "solo-3v3", 7).await;
        match tab.next_event().await {
            Some(QueueMessage::Error { code, .. }) => assert_eq!(code, "replaced"),
            other => panic!("expected replaced, got {other:?}"),
        }
        match other_tab.next().await {
            Some(QueueMessage::Queued { waiting, .. }) => assert_eq!(waiting, 1),
            other => panic!("expected queued, got {other:?}"),
        }
        // Stopping the door tells queued players.
        let stopping = tokio::spawn(door.stop());
        match other_tab.next_event().await {
            Some(QueueMessage::Error { code, .. }) => assert_eq!(code, "shutdown"),
            other => panic!("expected shutdown, got {other:?}"),
        }
        stopping.await.unwrap().unwrap();
        server.stop().await.unwrap();
    }
}
