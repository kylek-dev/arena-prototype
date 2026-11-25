//! Chat over the queue WebSocket (planning/issues/chat.md): the door sets
//! the sender and decides who hears each channel; spectators never reach
//! players; floods, oversized text, spoofed senders and abuse are refused
//! or masked. Guests only here; public chat, whispers, blocks and mutes
//! need accounts and are tested in `identity.rs`.

use std::net::SocketAddr;
use std::time::Duration;

use arena_server::door::{self, DoorConfig, QueueMessage};
use arena_server::ticket::TicketKey;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

const WAIT: Duration = Duration::from_secs(5);
type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn door() -> door::DoorRunning {
    let mut config = DoorConfig::new(
        TicketKey::random(),
        "ws://127.0.0.1:1/play".into(),
        "http://localhost:8000/".into(),
    );
    config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
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
    // The first party announcement means the door knows this connection.
    until(&mut socket, |m| {
        matches!(m, QueueMessage::Party { .. }).then_some(())
    })
    .await;
    socket
}

async fn say(socket: &mut Socket, json: String) {
    socket.send(Message::text(json)).await.unwrap();
}

async fn post(socket: &mut Socket, channel: &str, text: &str) {
    let json = serde_json::json!({"type": "chat", "channel": channel, "text": text});
    say(socket, json.to_string()).await;
}

async fn until<T>(socket: &mut Socket, wanted: impl Fn(QueueMessage) -> Option<T>) -> T {
    loop {
        let frame = tokio::time::timeout(WAIT, socket.next())
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

/// The next chat message: (channel, from, text).
async fn heard(socket: &mut Socket) -> (String, String, String) {
    until(socket, |m| match m {
        QueueMessage::Chat {
            channel,
            from,
            text,
            ..
        } => Some((channel, from, text)),
        _ => None,
    })
    .await
}

async fn refused(socket: &mut Socket) -> String {
    until(socket, |m| match m {
        QueueMessage::Refused { code } => Some(code),
        _ => None,
    })
    .await
}

/// Whether any chat arrives within `ms`.
async fn hears_chat(socket: &mut Socket, ms: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(ms);
    while let Ok(Some(Ok(frame))) = tokio::time::timeout_at(deadline, socket.next()).await {
        if let Message::Text(text) = frame
            && matches!(serde_json::from_str(&text), Ok(QueueMessage::Chat { .. }))
        {
            return true;
        }
    }
    false
}

/// Puts `rest` into `first`'s party.
async fn party(first: &mut Socket, rest: &mut [&mut Socket]) {
    say(first, r#"{"type":"create_party"}"#.into()).await;
    let code = until(first, |m| match m {
        QueueMessage::Party {
            code: Some(code), ..
        } => Some(code),
        _ => None,
    })
    .await;
    for (joined, socket) in rest.iter_mut().enumerate() {
        say(
            socket,
            format!(r#"{{"type":"join_party","code":"{code}"}}"#),
        )
        .await;
        let members = joined + 2;
        until(first, |m| {
            matches!(m, QueueMessage::Party { members: n, .. } if n == members).then_some(())
        })
        .await;
    }
}

#[tokio::test]
async fn the_door_names_the_sender_and_cleans_masks_and_bounds_text() {
    let door = door().await;
    let mut a = connect(door.addr, 1).await;
    let mut b = connect(door.addr, 2).await;
    let mut c = connect(door.addr, 3).await;
    party(&mut a, &mut [&mut b, &mut c]).await;
    post(&mut a, "team", "  gg  ").await;
    let (channel, from, text) = heard(&mut a).await;
    assert_eq!((channel.as_str(), text.as_str()), ("team", "gg"));
    assert!(from.starts_with("Guest#"), "{from}");
    for other in [&mut b, &mut c] {
        assert_eq!(
            heard(other).await,
            (channel.clone(), from.clone(), text.clone())
        );
    }

    // Abuse is masked; oversized, empty and unknown-channel posts refused.
    post(&mut a, "team", "nice shot you SH1T").await;
    assert_eq!(heard(&mut b).await.2, "nice shot you ****");
    post(&mut a, "team", &"x".repeat(201)).await;
    assert_eq!(refused(&mut a).await, "too_long");
    post(&mut a, "team", " \n ").await;
    assert_eq!(refused(&mut a).await, "empty");
    post(&mut a, "system", "server restarting").await;
    assert_eq!(refused(&mut a).await, "no_such_channel");
    // Guests have nothing to mute or block, so public needs a login.
    post(&mut b, "public", "hello world").await;
    assert_eq!(refused(&mut b).await, "log_in");
    assert_eq!(heard(&mut c).await.2, "nice shot you ****");
    assert!(!hears_chat(&mut c, 200).await, "refused posts reach nobody");

    // The cap is characters, and a full message fits in any script.
    tokio::time::sleep(Duration::from_secs(1)).await;
    for long in ["漢".repeat(200), "🙂".repeat(200)] {
        post(&mut c, "team", &long).await;
        assert_eq!(heard(&mut b).await.2, long);
    }
    door.stop().await.unwrap();
}

#[tokio::test]
async fn senders_cannot_be_spoofed_and_floods_are_cut() {
    let door = door().await;
    let mut a = connect(door.addr, 1).await;
    let mut b = connect(door.addr, 2).await;
    let mut c = connect(door.addr, 3).await;
    party(&mut b, &mut [&mut a, &mut c]).await;
    // Naming yourself is an unknown field: the door closes the connection.
    say(
        &mut a,
        r#"{"type":"chat","channel":"team","text":"hi","from":"Admin#0001","admin":true}"#.into(),
    )
    .await;
    let error = until(&mut a, |m| match m {
        QueueMessage::Error { code, .. } => Some(code),
        _ => None,
    })
    .await;
    assert_eq!(error, "bad_request");
    assert!(!hears_chat(&mut b, 200).await);

    // Five at once, then one a second.
    for n in 0..5 {
        post(&mut c, "team", &format!("message {n}")).await;
    }
    post(&mut c, "team", "one too many").await;
    assert_eq!(refused(&mut c).await, "rate_limited");
    let mut texts = Vec::new();
    for _ in 0..5 {
        texts.push(heard(&mut b).await.2);
    }
    assert_eq!(texts.last().map(String::as_str), Some("message 4"));
    assert!(!hears_chat(&mut b, 200).await, "the sixth never went out");
    door.stop().await.unwrap();
}

#[tokio::test]
async fn ignoring_a_player_silences_them_for_you_alone() {
    let door = door().await;
    let mut a = connect(door.addr, 1).await;
    let mut b = connect(door.addr, 2).await;
    let mut c = connect(door.addr, 3).await;
    party(&mut a, &mut [&mut b, &mut c]).await;
    post(&mut a, "team", "hello").await;
    let (_, a_handle, _) = heard(&mut a).await;
    heard(&mut b).await;
    heard(&mut c).await;
    say(
        &mut b,
        serde_json::json!({"type": "ignore", "handle": a_handle.to_lowercase()}).to_string(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    post(&mut a, "team", "again").await;
    assert_eq!(heard(&mut c).await.2, "again");
    assert!(!hears_chat(&mut b, 200).await, "ignored");
    say(
        &mut b,
        serde_json::json!({"type": "unignore", "handle": a_handle}).to_string(),
    )
    .await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    post(&mut a, "team", "back").await;
    assert_eq!(heard(&mut b).await.2, "back");
    // Nobody by that name, or a name far too long: refused, still connected.
    for handle in ["Nobody#0001".to_owned(), "x".repeat(300)] {
        say(
            &mut b,
            serde_json::json!({"type": "ignore", "handle": handle}).to_string(),
        )
        .await;
        assert_eq!(refused(&mut b).await, "no_such_player");
    }
    door.stop().await.unwrap();
}

#[tokio::test]
async fn team_chat_outside_a_match_is_the_party() {
    let door = door().await;
    let mut a = connect(door.addr, 1).await;
    let mut b = connect(door.addr, 2).await;
    let mut c = connect(door.addr, 3).await;
    say(&mut a, r#"{"type":"create_party"}"#.into()).await;
    let code = until(&mut a, |m| match m {
        QueueMessage::Party {
            code: Some(code), ..
        } => Some(code),
        _ => None,
    })
    .await;
    say(
        &mut b,
        format!(r#"{{"type":"join_party","code":"{code}"}}"#),
    )
    .await;
    until(&mut a, |m| {
        matches!(m, QueueMessage::Party { members: 2, .. }).then_some(())
    })
    .await;
    post(&mut a, "team", "push left").await;
    assert_eq!(heard(&mut b).await.2, "push left");
    assert!(!hears_chat(&mut c, 200).await, "not in the party");
    // Outside a match there is no match or spectator channel.
    post(&mut a, "match", "x").await;
    assert_eq!(refused(&mut a).await, "not_in_match");
    post(&mut a, "spectator", "x").await;
    assert_eq!(refused(&mut a).await, "not_spectating");
    door.stop().await.unwrap();
}

#[tokio::test]
async fn spectators_and_players_never_hear_each_other_in_a_match() {
    let door = door().await;
    let mut host = connect(door.addr, 1).await;
    let mut rival = connect(door.addr, 2).await;
    let mut watcher = connect(door.addr, 3).await;
    let mut outsider = connect(door.addr, 4).await;
    say(&mut host, r#"{"type":"create_lobby","mode":"1v1"}"#.into()).await;
    let code = until(&mut host, |m| match m {
        QueueMessage::Lobby(view) => Some(view.code),
        _ => None,
    })
    .await;
    for socket in [&mut rival, &mut watcher] {
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
    // The lobby talks among itself before the match.
    post(&mut watcher, "team", "good luck").await;
    assert_eq!(heard(&mut host).await.2, "good luck");
    assert_eq!(heard(&mut rival).await.2, "good luck");
    say(&mut host, r#"{"type":"start_lobby"}"#.into()).await;
    for socket in [&mut host, &mut rival, &mut watcher] {
        until(socket, |m| {
            matches!(m, QueueMessage::Match { .. }).then_some(())
        })
        .await;
    }

    // Match chat: both players, not the spectator or anyone else.
    post(&mut host, "match", "glhf").await;
    assert_eq!(heard(&mut rival).await.2, "glhf");
    assert_eq!(heard(&mut host).await.2, "glhf");
    // Team chat in a 1v1 is just you.
    post(&mut rival, "team", "they are low").await;
    assert_eq!(heard(&mut rival).await.2, "they are low");
    assert!(
        !hears_chat(&mut host, 200).await,
        "the other team never hears it"
    );
    // Spectators only talk among themselves; players never hear them.
    post(&mut watcher, "spectator", "host is behind the pillar").await;
    assert_eq!(heard(&mut watcher).await.2, "host is behind the pillar");
    for channel in ["public", "match", "team"] {
        post(&mut watcher, channel, "psst").await;
        assert_eq!(refused(&mut watcher).await, "spectators_only", "{channel}");
    }
    post(&mut host, "spectator", "x").await;
    assert_eq!(refused(&mut host).await, "not_spectating");
    assert!(!hears_chat(&mut host, 200).await);
    assert!(!hears_chat(&mut rival, 200).await);
    assert!(!hears_chat(&mut outsider, 200).await);

    // A second tab joining the (still open) lobby mid-match reaches the
    // lobby's spectator, never its players.
    let mut late = connect(door.addr, 5).await;
    say(
        &mut late,
        format!(r#"{{"type":"join_lobby","code":"{code}"}}"#),
    )
    .await;
    until(&mut late, |m| {
        matches!(m, QueueMessage::Lobby(_)).then_some(())
    })
    .await;
    post(&mut late, "team", "host is behind the pillar").await;
    assert_eq!(heard(&mut watcher).await.2, "host is behind the pillar");
    assert!(
        !hears_chat(&mut host, 200).await,
        "players hear only their match"
    );
    assert!(!hears_chat(&mut rival, 200).await);
    door.stop().await.unwrap();
}

#[tokio::test]
async fn team_chat_in_a_2v2_reaches_one_teammate() {
    let door = door().await;
    let mut host = connect(door.addr, 1).await;
    let mut others = vec![
        connect(door.addr, 2).await,
        connect(door.addr, 3).await,
        connect(door.addr, 4).await,
    ];
    say(&mut host, r#"{"type":"create_lobby","mode":"2v2"}"#.into()).await;
    let code = until(&mut host, |m| match m {
        QueueMessage::Lobby(view) => Some(view.code),
        _ => None,
    })
    .await;
    for socket in &mut others {
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
    say(&mut host, r#"{"type":"start_lobby"}"#.into()).await;
    until(&mut host, |m| {
        matches!(m, QueueMessage::Match { .. }).then_some(())
    })
    .await;
    for socket in &mut others {
        until(socket, |m| {
            matches!(m, QueueMessage::Match { .. }).then_some(())
        })
        .await;
    }
    post(&mut host, "team", "with me").await;
    assert_eq!(heard(&mut host).await.2, "with me");
    let mut heard_by = 0;
    for socket in &mut others {
        heard_by += usize::from(hears_chat(socket, 300).await);
    }
    assert_eq!(heard_by, 1, "one teammate, neither enemy");
    post(&mut host, "match", "glhf").await;
    for socket in &mut others {
        assert_eq!(heard(socket).await.2, "glhf");
    }
    door.stop().await.unwrap();
}
