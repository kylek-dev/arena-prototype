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
            appearance: None,
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

    struct Player {
        sink: SplitSink<Socket, Message>,
        stream: SplitStream<Socket>,
    }
    impl Player {
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
        /// Connected and on their own.
        async fn hello(addr: SocketAddr, guest: u128) -> Self {
            let mut player = Self::open(addr).await;
            player
                .say(&format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#))
                .await;
            match player.next().await {
                Some(QueueMessage::Party { members: 1, .. }) => {}
                other => panic!("expected their own party, got {other:?}"),
            }
            player
        }
        async fn queue(&mut self, mode: &str) {
            self.say(&format!(r#"{{"type":"queue","mode":"{mode}"}}"#))
                .await;
        }
        /// Connected and searching on their own.
        async fn searching(addr: SocketAddr, mode: &str, guest: u128) -> Self {
            let mut player = Self::hello(addr, guest).await;
            player.queue(mode).await;
            player
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
        /// Skips party descriptions and search status.
        async fn next_event(&mut self) -> Option<QueueMessage> {
            loop {
                match self.next().await {
                    Some(QueueMessage::Queued { .. } | QueueMessage::Party { .. }) => {}
                    other => return other,
                }
            }
        }
        /// The next party description.
        async fn party(&mut self) -> QueueMessage {
            loop {
                match self.next().await {
                    Some(party @ QueueMessage::Party { .. }) => return party,
                    Some(_) => {}
                    None => panic!("closed while waiting for a party update"),
                }
            }
        }
        async fn error(&mut self) -> String {
            match self.next_event().await {
                Some(QueueMessage::Error { code, .. }) => code,
                other => panic!("expected an error, got {other:?}"),
            }
        }
        /// Confirms the ready check the door starts.
        async fn confirm(&mut self) {
            self.offered().await;
            self.say(r#"{"type":"accept"}"#).await;
        }
        /// Waits for a match to be offered; returns how many must accept.
        async fn offered(&mut self) -> usize {
            match self.next_event().await {
                Some(QueueMessage::ReadyCheck {
                    accepted: 0,
                    total,
                    you: false,
                    remaining_ms,
                    ..
                }) => {
                    assert!(remaining_ms > 0);
                    total
                }
                other => panic!("expected a ready check, got {other:?}"),
            }
        }
        /// The next message that is not a ready check's progress.
        async fn after_check(&mut self) -> Option<QueueMessage> {
            loop {
                match self.next_event().await {
                    Some(QueueMessage::ReadyCheck { .. }) => {}
                    other => return other,
                }
            }
        }
        /// The seat given once everyone accepted.
        async fn seat(&mut self) -> (usize, String, String) {
            match self.after_check().await {
                Some(QueueMessage::Match {
                    seat,
                    server,
                    ticket,
                    ..
                }) => (seat, server, ticket),
                other => panic!("expected a match, got {other:?}"),
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

    /// Joins the match server with a seat ticket; returns the actor welcomed.
    async fn join_match(server: &str, ticket: &str) -> (Socket, u8) {
        let (mut socket, _) = connect_async(server).await.unwrap();
        let hello = ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket: from_hex(ticket).unwrap(),
            appearance: None,
        };
        socket.send(Message::binary(hello.encode())).await.unwrap();
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        match ServerMessage::decode(&frame.into_data()).unwrap() {
            ServerMessage::Welcome(welcome) => (socket, welcome.actor),
            other => panic!("expected a welcome, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn four_solo_players_are_matched_and_each_joins_their_own_seat() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        let mut players = Vec::new();
        for guest in 1..=3u128 {
            let mut player = Player::searching(door.addr, "2v2", guest).await;
            loop {
                match player.next().await {
                    Some(QueueMessage::Queued {
                        mode,
                        waiting,
                        waited_ms,
                        expected_ms,
                    }) => {
                        assert_eq!((mode.as_str(), waiting), ("2v2", guest as usize));
                        assert!(waited_ms < 2000, "{waited_ms}");
                        // No matches yet: nothing to estimate from.
                        assert_eq!(expected_ms, None);
                        break;
                    }
                    Some(QueueMessage::Party { .. }) => {}
                    other => panic!("expected queued, got {other:?}"),
                }
            }
            players.push(player);
        }
        players.push(Player::searching(door.addr, "2v2", 4).await);
        for player in &mut players {
            player.confirm().await;
        }
        let mut actors = Vec::new();
        let mut sockets = Vec::new();
        for player in &mut players {
            let (seat, play, ticket) = player.seat().await;
            // Still connected and on their own, ready to queue again.
            assert!(matches!(
                player.next().await,
                Some(QueueMessage::Party {
                    members: 1,
                    queue: None,
                    ..
                })
            ));
            let (socket, actor) = join_match(&play, &ticket).await;
            assert_eq!(usize::from(actor), seat);
            actors.push(actor);
            sockets.push(socket);
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
    async fn friends_queue_as_a_party_and_play_on_one_team() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        let mut leader = Player::hello(door.addr, 1).await;
        leader.say(r#"{"type":"create_party"}"#).await;
        let QueueMessage::Party {
            code: Some(code),
            members: 1,
            leader: true,
            ..
        } = leader.party().await
        else {
            panic!("expected an invite code");
        };
        let mut friend = Player::hello(door.addr, 2).await;
        friend
            .say(&format!(r#"{{"type":"join_party","code":"{code}"}}"#))
            .await;
        assert!(matches!(
            friend.party().await,
            QueueMessage::Party {
                members: 2,
                leader: false,
                ..
            }
        ));
        assert!(matches!(
            leader.party().await,
            QueueMessage::Party { members: 2, .. }
        ));
        // Only the leader queues the party; a solo queue cannot take it.
        friend.queue("2v2").await;
        assert_eq!(
            friend.next_event().await,
            Some(QueueMessage::Refused {
                code: "not_leader".into()
            })
        );
        leader.queue("1v1").await;
        assert_eq!(
            leader.next_event().await,
            Some(QueueMessage::Refused {
                code: "party_too_big".into()
            })
        );
        leader.queue("2v2").await;
        // Two strangers on their own complete the match.
        let mut others = vec![
            Player::searching(door.addr, "2v2", 3).await,
            Player::searching(door.addr, "2v2", 4).await,
        ];
        leader.confirm().await;
        friend.confirm().await;
        for other in &mut others {
            other.confirm().await;
        }
        let (leader_seat, play, ticket) = leader.seat().await;
        let (friend_seat, _, friend_ticket) = friend.seat().await;
        assert_eq!(leader_seat % 2, friend_seat % 2, "friends share a team");
        let mut sockets = vec![
            join_match(&play, &ticket).await.0,
            join_match(&play, &friend_ticket).await.0,
        ];
        for other in &mut others {
            let (seat, _, ticket) = other.seat().await;
            assert_ne!(seat % 2, leader_seat % 2);
            sockets.push(join_match(&play, &ticket).await.0);
        }
        assert_eq!(server.active_matches(), 1);
        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn players_who_do_not_accept_leave_the_queue_and_the_others_go_first() {
        let (door, server) = door_and_server(Duration::from_millis(300)).await;
        let mut first = Player::searching(door.addr, "1v1", 1).await;
        let mut absent = Player::searching(door.addr, "1v1", 2).await;
        assert_eq!(first.offered().await, 2);
        assert_eq!(absent.offered().await, 2);
        first.say(r#"{"type":"accept"}"#).await;
        // Everyone sees the acceptance.
        assert!(matches!(
            absent.next_event().await,
            Some(QueueMessage::ReadyCheck {
                accepted: 1,
                total: 2,
                you: false,
                ..
            })
        ));
        // The absent player never accepts: out of the queue, still connected.
        assert_eq!(
            absent.after_check().await,
            Some(QueueMessage::Stopped {
                reason: "not_accepted".into()
            })
        );
        assert_eq!(
            first.after_check().await,
            Some(QueueMessage::Requeued { mode: "1v1".into() })
        );
        // A newcomer is matched with the requeued player straight away.
        let mut newcomer = Player::searching(door.addr, "1v1", 3).await;
        first.offered().await;
        newcomer.offered().await;
        // The absent player can queue again on the same connection.
        absent.queue("1v1").await;
        loop {
            match absent.next().await {
                Some(QueueMessage::Queued { .. }) => break,
                Some(_) => {}
                None => panic!("the connection closed"),
            }
        }
        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn declining_ends_the_check_for_everyone_at_once() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        let mut players = Vec::new();
        for guest in 1..=4u128 {
            players.push(Player::searching(door.addr, "2v2", guest).await);
        }
        for player in &mut players {
            assert_eq!(player.offered().await, 4);
        }
        players[0].say(r#"{"type":"accept"}"#).await;
        players[1].say(r#"{"type":"decline"}"#).await;
        assert_eq!(
            players[1].after_check().await,
            Some(QueueMessage::Stopped {
                reason: "declined".into()
            })
        );
        assert_eq!(
            players[0].after_check().await,
            Some(QueueMessage::Requeued { mode: "2v2".into() })
        );
        for player in &mut players[2..] {
            assert_eq!(
                player.after_check().await,
                Some(QueueMessage::Stopped {
                    reason: "not_accepted".into()
                })
            );
        }
        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn the_queue_refuses_strangers_bad_messages_and_second_tabs() {
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
            r#"{"type":"queue","mode":"1v1"}"#,
            r#"{"type":"hello","guest":"me"}"#,
            "hello",
        ] {
            let mut player = Player::open(door.addr).await;
            player.say(bad).await;
            assert_eq!(player.error().await, "bad_request", "{bad}");
        }
        let mut player = Player::hello(door.addr, 9).await;
        player.queue("4v4").await;
        assert_eq!(player.error().await, "bad_request");
        // An unknown invite code is refused without closing.
        let mut player = Player::hello(door.addr, 8).await;
        player.say(r#"{"type":"join_party","code":"ZZZZZZ"}"#).await;
        assert_eq!(
            player.next_event().await,
            Some(QueueMessage::Refused {
                code: "no_such_party".into()
            })
        );
        // Opening the game in another tab replaces the first connection.
        let mut tab = Player::searching(door.addr, "3v3", 7).await;
        let mut other_tab = Player::hello(door.addr, 7).await;
        assert_eq!(tab.error().await, "replaced");
        other_tab.queue("3v3").await;
        loop {
            match other_tab.next().await {
                Some(QueueMessage::Queued { waiting, .. }) => {
                    assert_eq!(waiting, 1);
                    break;
                }
                Some(QueueMessage::Party { .. }) => {}
                other => panic!("expected queued, got {other:?}"),
            }
        }
        // Stopping the door tells connected players.
        let stopping = tokio::spawn(door.stop());
        assert_eq!(other_tab.error().await, "shutdown");
        stopping.await.unwrap().unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn the_queue_rate_limits_websocket_control_ping_floods() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        // Pre-hello control Ping flood is rate-limited (10/s budget -> 11th trips it).
        let mut pre = Player::open(door.addr).await;
        for byte in 0..11u8 {
            if pre
                .sink
                .send(Message::Ping(vec![byte].into()))
                .await
                .is_err()
            {
                break;
            }
        }
        assert_eq!(pre.error().await, "rate_limited");

        // Post-hello control Ping flood is rate-limited.
        let mut joined = Player::hello(door.addr, 42).await;
        for byte in 0..11u8 {
            if joined
                .sink
                .send(Message::Ping(vec![byte].into()))
                .await
                .is_err()
            {
                break;
            }
        }
        assert_eq!(joined.error().await, "rate_limited");

        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }

    #[tokio::test]
    async fn custom_lobby_hosts_moves_enforces_settings_starts_and_rematches() {
        let (door, server) = door_and_server(Duration::from_secs(5)).await;
        let mut host = Player::hello(door.addr, 101).await;
        let mut guest = Player::hello(door.addr, 102).await;
        let mut spec = Player::hello(door.addr, 103).await;

        // Host creates a 1v1 custom lobby.
        host.say(r#"{"type":"create_lobby","mode":"1v1"}"#).await;
        let code = match host.next_event().await {
            Some(QueueMessage::Lobby(view)) => {
                assert!(view.host);
                assert!(!view.can_start);
                view.code
            }
            other => panic!("expected lobby, got {other:?}"),
        };

        // Host cannot start until both teams are full.
        host.say(r#"{"type":"start_lobby"}"#).await;
        assert_eq!(
            host.next_event().await,
            Some(QueueMessage::Refused {
                code: "lobby_not_full".into()
            })
        );

        // Guest joins by code -> fills Team B; spectator joins -> Spectator slot.
        guest
            .say(&format!(r#"{{"type":"join_lobby","code":"{code}"}}"#))
            .await;
        let _ = host.next_event().await;
        match guest.next_event().await {
            Some(QueueMessage::Lobby(view)) => {
                assert!(view.can_start);
                assert!(!view.host);
            }
            other => panic!("expected lobby, got {other:?}"),
        }
        spec.say(&format!(r#"{{"type":"join_lobby","code":"{code}"}}"#))
            .await;
        let _ = host.next_event().await;
        let _ = guest.next_event().await;
        match spec.next_event().await {
            Some(QueueMessage::Lobby(view)) => {
                assert_eq!(view.members.len(), 3);
                assert_eq!(
                    view.members[2].slot,
                    arena_server::matchmaker::LobbySlot::Spectator
                );
            }
            other => panic!("expected lobby for spectator, got {other:?}"),
        }

        // Host enforces a 5-minute (300 s) custom round time limit.
        host.say(r#"{"type":"set_lobby_settings","settings":{"version":1,"map":"colosseum","round_seconds":300}}"#)
            .await;
        for p in [&mut host, &mut guest, &mut spec] {
            match p.next_event().await {
                Some(QueueMessage::Lobby(view)) => assert_eq!(view.settings.round_seconds, 300),
                other => panic!("expected updated lobby settings, got {other:?}"),
            }
        }

        // Start match 1: seated players get seat tickets, spectator gets signed spectator ticket!
        host.say(r#"{"type":"start_lobby"}"#).await;
        let (first_match_id, host_ticket, server_url) = match host.next_event().await {
            Some(QueueMessage::Match {
                mode,
                match_id,
                seat,
                spectator,
                server,
                ticket,
                ..
            }) => {
                assert_eq!(mode, "1v1");
                assert_eq!(seat, 0);
                assert!(!spectator);
                (match_id, ticket, server)
            }
            other => panic!("expected match for host, got {other:?}"),
        };
        let guest_ticket = match guest.next_event().await {
            Some(QueueMessage::Match {
                match_id,
                seat,
                spectator,
                ticket,
                ..
            }) => {
                assert_eq!(match_id, first_match_id);
                assert_eq!(seat, 1);
                assert!(!spectator);
                ticket
            }
            other => panic!("expected match for guest, got {other:?}"),
        };
        let spec_ticket = match spec.next_event().await {
            Some(QueueMessage::Match {
                mode,
                match_id,
                spectator,
                ticket,
                ..
            }) => {
                assert_eq!(mode, "1v1");
                assert_eq!(match_id, first_match_id);
                assert!(spectator);
                ticket
            }
            other => panic!("expected spectator match ticket, got {other:?}"),
        };

        // Connecting to the real match server verifies the signed custom round time limit
        // AND accepts the signed spectator ticket as watch-only!
        let mut live_sockets = Vec::new();
        for (seat, ticket) in [(0u8, host_ticket), (1u8, guest_ticket)] {
            let (mut socket, _) = connect_async(&server_url).await.unwrap();
            let hello = ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                ticket: from_hex(&ticket).unwrap(),
                appearance: None,
            };
            socket.send(Message::binary(hello.encode())).await.unwrap();
            let welcome = tokio::time::timeout(WAIT, socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match ServerMessage::decode(&welcome.into_data()).unwrap() {
                ServerMessage::Welcome(welcome) => {
                    assert_eq!(welcome.actor, seat);
                    assert!(!welcome.spectator);
                    assert_eq!(
                        welcome.round_ticks,
                        300 * arena_sim::TICKS_PER_SECOND,
                        "custom 5-minute round limit must be enforced by the match server"
                    );
                }
                other => panic!("expected Welcome, got {other:?}"),
            }
            live_sockets.push(socket);
        }
        {
            let (mut spec_socket, _) = connect_async(&server_url).await.unwrap();
            let hello = ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                ticket: from_hex(&spec_ticket).unwrap(),
                appearance: None,
            };
            spec_socket
                .send(Message::binary(hello.encode()))
                .await
                .unwrap();
            let welcome = tokio::time::timeout(WAIT, spec_socket.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            match ServerMessage::decode(&welcome.into_data()).unwrap() {
                ServerMessage::Welcome(welcome) => {
                    assert!(welcome.spectator);
                    assert_eq!(welcome.actor, 0);
                    assert_eq!(welcome.round_ticks, 300 * arena_sim::TICKS_PER_SECOND);
                }
                other => panic!("expected spectator Welcome, got {other:?}"),
            }
            live_sockets.push(spec_socket);
        }
        drop(live_sockets);

        // Lobby invite code is never a match seat credential: /play rejects it with BadTicket.
        {
            let (mut raw, _) = connect_async(&server_url).await.unwrap();
            let forged = ClientMessage::Hello {
                protocol: PROTOCOL_VERSION,
                ticket: code.as_bytes().to_vec(),
                appearance: None,
            };
            raw.send(Message::binary(forged.encode())).await.unwrap();
            let reply = tokio::time::timeout(WAIT, raw.next())
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert_eq!(
                ServerMessage::decode(&reply.into_data()).unwrap(),
                ServerMessage::Error(arena_protocol::ErrorCode::BadTicket)
            );
        }

        // Rematch: host starts a second match in the same lobby with no new invites!
        host.say(r#"{"type":"start_lobby"}"#).await;
        let second_match_id = match host.next_event().await {
            Some(QueueMessage::Match { match_id, .. }) => match_id,
            other => panic!("expected rematch ticket, got {other:?}"),
        };
        assert_ne!(second_match_id, first_match_id);
        match guest.next_event().await {
            Some(QueueMessage::Match { match_id, .. }) => assert_eq!(match_id, second_match_id),
            other => panic!("expected rematch ticket for guest, got {other:?}"),
        }
        match spec.next_event().await {
            Some(QueueMessage::Match {
                mode,
                match_id,
                spectator,
                ..
            }) => {
                assert_eq!(mode, "1v1");
                assert_eq!(match_id, second_match_id);
                assert!(spectator);
            }
            other => panic!("expected spectator rematch ticket, got {other:?}"),
        }

        // Per-address match creation rate limit applies to start_lobby too (default burst = 5).
        for _ in 0..3 {
            host.say(r#"{"type":"start_lobby"}"#).await;
            assert!(matches!(
                host.next_event().await,
                Some(QueueMessage::Match {
                    spectator: false,
                    ..
                })
            ));
            assert!(matches!(
                guest.next_event().await,
                Some(QueueMessage::Match {
                    spectator: false,
                    ..
                })
            ));
            assert!(matches!(
                spec.next_event().await,
                Some(QueueMessage::Match {
                    spectator: true,
                    ..
                })
            ));
        }
        host.say(r#"{"type":"start_lobby"}"#).await;
        assert_eq!(
            host.next_event().await,
            Some(QueueMessage::Refused {
                code: "rate_limited".into()
            })
        );

        let metrics = request(door.addr, "GET", "/metrics", "", "").await.body;
        assert!(
            metrics.contains("arena_door_custom_matches_total 5\n"),
            "{metrics}"
        );
        assert!(
            metrics.contains("arena_door_refused_rate_total 1\n"),
            "{metrics}"
        );

        door.stop().await.unwrap();
        server.stop().await.unwrap();
    }
}
