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
