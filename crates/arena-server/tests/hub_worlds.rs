//! The front door places players in hub worlds on the world server (ADR
//! 0057 slice 4): `world_join` over the queue socket answers a `world`
//! ticket once the directory exchange after the placement has answered;
//! the page joins the world with it; players who leave the world are taken
//! out of the door's placement.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use arena_server::door::{self, DoorConfig, QueueMessage};
use arena_server::ticket::{TicketKey, from_hex};
use arena_server::{Config, Running, start};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const WAIT: Duration = Duration::from_secs(8);
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn world_server(key: TicketKey) -> Running {
    let mut config = Config::new(key);
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.tick = Duration::from_millis(5);
    config.connections_per_address = 0;
    config.world_limits.grace = 4;
    start(config).await.unwrap()
}

async fn door_for(server: Option<&Running>, key: TicketKey) -> door::DoorRunning {
    let play = server.map_or("ws://127.0.0.1:1/play".into(), |server| {
        format!("ws://{}/play", server.addr)
    });
    let mut config = DoorConfig::new(key, play, "http://localhost:8000/".into());
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    config.match_status = server.map(|server| format!("http://{}", server.addr));
    door::start(config).await.unwrap()
}

async fn connect(door: SocketAddr, guest: u128) -> Socket {
    let (mut socket, _) = connect_async(format!("ws://{door}/api/queue"))
        .await
        .unwrap();
    say(
        &mut socket,
        format!(r#"{{"type":"hello","guest":"{guest:032x}"}}"#),
    )
    .await;
    until(&mut socket, |m| {
        matches!(m, QueueMessage::Party { .. }).then_some(())
    })
    .await;
    socket
}

async fn say(socket: &mut Socket, json: String) {
    socket.send(Message::text(json)).await.unwrap();
}

async fn until<T>(socket: &mut Socket, wanted: impl Fn(QueueMessage) -> Option<T>) -> T {
    until_within(WAIT, socket, wanted).await
}

async fn until_within<T>(
    wait: Duration,
    socket: &mut Socket,
    wanted: impl Fn(QueueMessage) -> Option<T>,
) -> T {
    loop {
        let frame = tokio::time::timeout(wait, socket.next())
            .await
            .expect("an answer in time")
            .expect("still open")
            .unwrap();
        let Message::Text(text) = frame else { continue };
        if let Some(found) = wanted(serde_json::from_str(&text).unwrap()) {
            return found;
        }
    }
}

/// Asks for a world; returns (instance, server, ticket).
async fn world_ticket(socket: &mut Socket) -> (String, String, String) {
    say(socket, r#"{"type":"world_join"}"#.into()).await;
    until(socket, |m| match m {
        QueueMessage::World {
            instance,
            server,
            ticket,
            ..
        } => Some((instance, server, ticket)),
        QueueMessage::Refused { code } => panic!("refused: {code}"),
        _ => None,
    })
    .await
}

type WorldSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn enter(server: &str, ticket: &str) -> WorldSocket {
    let (mut socket, _) = connect_async(server).await.unwrap();
    let hello = ClientMessage::Hello {
        protocol: PROTOCOL_VERSION,
        ticket: from_hex(ticket).unwrap(),
        appearance: None,
    };
    socket.send(Message::binary(hello.encode())).await.unwrap();
    socket
}

async fn next_world(socket: &mut WorldSocket) -> ServerMessage {
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .expect("an answer in time")
            .expect("still open")
            .unwrap();
        if let Message::Binary(frame) = frame {
            return ServerMessage::decode(&frame).unwrap();
        }
    }
}

#[tokio::test]
async fn the_door_places_players_together_and_they_meet_in_the_world() {
    let key = TicketKey::random();
    let server = world_server(key.clone()).await;
    let door = door_for(Some(&server), key).await;
    let mut ann = connect(door.addr, 1).await;
    let mut bo = connect(door.addr, 2).await;

    let (instance, play, ticket) = world_ticket(&mut ann).await;
    assert_eq!(play, format!("ws://{}/play", server.addr));
    let mut ann_world = enter(&play, &ticket).await;
    let first = next_world(&mut ann_world).await;
    let ServerMessage::Welcome(welcome) = first else {
        panic!("welcome first, got {first:?}");
    };
    assert!(welcome.hub);

    // Asking twice while placed is refused.
    say(&mut ann, r#"{"type":"world_join"}"#.into()).await;
    let code = until(&mut ann, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(code, "in_hub");

    // The second player goes to the same (fullest) world.
    let (second, play, ticket) = world_ticket(&mut bo).await;
    assert_eq!(second, instance);
    let mut bo_world = enter(&play, &ticket).await;
    loop {
        if let ServerMessage::Snapshot(snapshot) = next_world(&mut bo_world).await {
            assert_eq!(snapshot.hub_arena().unwrap().players(), 2);
            break;
        }
    }

    // Leaving: the door says so.
    say(&mut bo, r#"{"type":"world_leave"}"#.into()).await;
    let reason = until(&mut bo, |m| match m {
        QueueMessage::HubLeft { reason } => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, "left");
    drop(bo_world);
    drop(ann_world);
}

#[tokio::test]
async fn players_whose_world_connection_ends_are_taken_out_of_the_hub() {
    let key = TicketKey::random();
    let server = world_server(key.clone()).await;
    let door = door_for(Some(&server), key).await;
    let mut ann = connect(door.addr, 3).await;
    let (_, play, ticket) = world_ticket(&mut ann).await;
    let mut world = enter(&play, &ticket).await;
    // Seen in the world (the next directory exchange confirms it), then
    // gone. Should the exchange miss them, the placement still ends when
    // its confirmation window does: wait past it.
    let _ = next_world(&mut world).await;
    tokio::time::sleep(Duration::from_millis(1_500)).await;
    let _ = world.close(None).await;
    let reason = until_within(Duration::from_secs(30), &mut ann, |m| match m {
        QueueMessage::HubLeft { reason } => Some(reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, "left");
    // And may come back.
    let (_, _, again) = world_ticket(&mut ann).await;
    assert!(!again.is_empty());
}

#[tokio::test]
async fn without_a_world_server_status_the_ticket_comes_at_once() {
    let door = door_for(None, TicketKey::random()).await;
    let mut ann = connect(door.addr, 4).await;
    let (instance, play, ticket) = world_ticket(&mut ann).await;
    assert_eq!(instance.len(), 16);
    assert_eq!(play, "ws://127.0.0.1:1/play");
    assert!(!ticket.is_empty());
}

#[tokio::test]
async fn leaving_the_hub_takes_the_player_out_of_the_world_too() {
    let key = TicketKey::random();
    let server = world_server(key.clone()).await;
    let door = door_for(Some(&server), key).await;
    let mut ann = connect(door.addr, 5).await;
    let (_, play, ticket) = world_ticket(&mut ann).await;
    let mut world = enter(&play, &ticket).await;
    let _ = next_world(&mut world).await;
    say(&mut ann, r#"{"type":"world_leave"}"#.into()).await;
    until(&mut ann, |m| {
        matches!(m, QueueMessage::HubLeft { .. }).then_some(())
    })
    .await;
    // The next directory exchange no longer lists them: the world closes
    // their connection, and the old ticket lets nobody back in.
    let code = loop {
        if let ServerMessage::Error(code) = next_world(&mut world).await {
            break code;
        }
    };
    assert_eq!(code, arena_protocol::ErrorCode::MatchOver);
    let mut again = enter(&play, &ticket).await;
    let code = loop {
        if let ServerMessage::Error(code) = next_world(&mut again).await {
            break code;
        }
    };
    assert_eq!(code, arena_protocol::ErrorCode::BadTicket);
}

#[tokio::test]
async fn following_a_stranger_is_refused() {
    let key = TicketKey::random();
    let server = world_server(key.clone()).await;
    let door = door_for(Some(&server), key).await;
    let mut ann = connect(door.addr, 6).await;
    say(
        &mut ann,
        r#"{"type":"world_join","friend":"Nobody#0001"}"#.into(),
    )
    .await;
    let code = until(&mut ann, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(code, "not_friends");
}
