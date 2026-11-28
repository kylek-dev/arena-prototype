//! Control-plane spans over real connections (ADR 0054): a queue search is
//! one trace from entering the queue to its ticket, a match is one span with
//! its seats as children, HTTP requests get a span but probes and scrapes do
//! not, and no span carries a ticket.

use std::net::SocketAddr;
use std::time::Duration;

use arena_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use arena_world::door::{self, DoorConfig, QueueMessage};
use arena_world::ticket::{TicketKey, from_hex};
use arena_world::{Config, start};
use futures_util::{SinkExt, StreamExt};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_sdk::trace::{InMemorySpanExporter, SdkTracerProvider, SpanData};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing_subscriber::Layer;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

const PAGE: &str = "http://localhost:8000/";
const WAIT: Duration = Duration::from_secs(5);

async fn get(addr: SocketAddr, path: &str) -> u16 {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    response[9..12].parse().unwrap()
}

/// Opens a queue connection as `guest` and sends `messages` after hello;
/// returns the socket and the first party invite code it hears, if any.
async fn queue_socket(
    door: SocketAddr,
    guest: u128,
    messages: &[&str],
) -> (impl Sized, Option<String>) {
    let (mut socket, _) = connect_async(format!("ws://{door}/api/queue"))
        .await
        .unwrap();
    socket
        .send(Message::text(format!(
            r#"{{"type":"hello","guest":"{guest:032x}"}}"#
        )))
        .await
        .unwrap();
    for message in messages {
        socket.send(Message::text(*message)).await.unwrap();
    }
    let mut code = None;
    let deadline = tokio::time::Instant::now() + Duration::from_millis(300);
    while let Ok(Some(Ok(Message::Text(text)))) =
        tokio::time::timeout_at(deadline, socket.next()).await
    {
        if let Ok(QueueMessage::Party {
            code: Some(found), ..
        }) = serde_json::from_str(&text)
        {
            code = Some(found);
        }
    }
    (socket, code)
}

/// Queues `guest` for a duel, accepts the check and returns its ticket.
async fn queue_for_a_ticket(door: SocketAddr, guest: u128) -> (impl Sized, String) {
    let (mut socket, _) = connect_async(format!("ws://{door}/api/queue"))
        .await
        .unwrap();
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
            QueueMessage::ReadyCheck { you: false, .. } => {
                socket
                    .send(Message::text(r#"{"type":"accept"}"#))
                    .await
                    .unwrap();
            }
            QueueMessage::Match { ticket, .. } => return (socket, ticket),
            _ => {}
        }
    }
}

async fn join(server: SocketAddr, ticket: &str) -> impl Sized {
    let (mut socket, _) = connect_async(format!("ws://{server}/play")).await.unwrap();
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
    assert!(matches!(
        ServerMessage::decode(&frame.into_data()).unwrap(),
        ServerMessage::Welcome(_)
    ));
    socket
}

fn attribute(span: &SpanData, key: &str) -> Option<String> {
    span.attributes
        .iter()
        .find(|pair| pair.key.as_str() == key)
        .map(|pair| pair.value.as_str().into_owned())
}

#[tokio::test]
async fn control_plane_steps_are_traced_and_carry_no_tickets() {
    let exporter = InMemorySpanExporter::default();
    let provider = SdkTracerProvider::builder()
        .with_simple_exporter(exporter.clone())
        .build();
    let subscriber = tracing_subscriber::registry().with(
        tracing_opentelemetry::layer()
            .with_tracer(provider.tracer("test"))
            .with_filter(LevelFilter::INFO),
    );
    // This test binary holds only this test, so the global default is ours.
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let key = TicketKey::random();
    let mut server_config = Config::new(key.clone());
    server_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    server_config.tick = Duration::from_millis(5);
    let server = start(server_config).await.unwrap();
    let mut door_config = DoorConfig::new(key, format!("ws://{}/play", server.addr), PAGE.into());
    door_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    let door = door::start(door_config).await.unwrap();

    assert_eq!(get(door.addr, "/metrics").await, 200);
    assert_eq!(get(server.addr, "/healthz").await, 200);
    assert_eq!(get(door.addr, "/lobby/").await, 200);
    // A query string is never recorded, nor an unknown path.
    assert_eq!(get(door.addr, "/lobby/?ticket=SECRETQUERY").await, 200);
    assert_eq!(get(door.addr, "/SECRETPATH").await, 404);
    // A guest who searches in 2v2 and then reconnects (another tab):
    // the first search ends as replaced.
    let (replaced, _) = queue_socket(door.addr, 9, &[r#"{"type":"queue","mode":"2v2"}"#]).await;
    let (again, _) = queue_socket(door.addr, 9, &[]).await;
    // A party: its invite code must not reach a span.
    let (party, invite) = queue_socket(door.addr, 10, &[r#"{"type":"create_party"}"#]).await;
    let invite = invite.expect("created parties have a code");
    let (first, second) = tokio::join!(
        queue_for_a_ticket(door.addr, 1),
        queue_for_a_ticket(door.addr, 2)
    );
    let tickets = [first.1.clone(), second.1.clone()];
    let seats = [
        join(server.addr, &tickets[0]).await,
        join(server.addr, &tickets[1]).await,
    ];
    tokio::time::sleep(Duration::from_millis(50)).await;
    drop(seats);
    drop((first, second, replaced, again, party));
    tokio::time::sleep(Duration::from_millis(100)).await;
    door.stop().await.unwrap();
    server.stop().await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    provider.force_flush().unwrap();

    let spans = exporter.get_finished_spans().unwrap();
    let named = |name: &'static str| spans.iter().filter(move |span| span.name == name);
    let summary = || {
        spans
            .iter()
            .map(|span| format!("{} {:?}", span.name, span.attributes))
            .collect::<Vec<_>>()
            .join("\n")
    };

    // HTTP: the lobby page and upgrades are traced; scrapes and probes not.
    let paths: Vec<String> = named("http.request")
        .filter_map(|span| attribute(span, "path"))
        .collect();
    assert!(paths.contains(&"/lobby/".to_owned()), "{paths:?}");
    assert!(paths.contains(&"/api/queue".to_owned()), "{paths:?}");
    assert!(paths.contains(&"/play".to_owned()), "{paths:?}");
    assert!(
        !paths
            .iter()
            .any(|path| path == "/metrics" || path == "/healthz"),
        "{paths:?}"
    );
    assert!(paths.contains(&"unmatched".to_owned()), "{paths:?}");
    let lobby = named("http.request")
        .find(|span| attribute(span, "path").as_deref() == Some("/lobby/"))
        .unwrap();
    assert_eq!(attribute(lobby, "status").as_deref(), Some("200"));

    // The replaced search is closed and says why.
    let outcomes: Vec<Option<String>> = named("queue.search")
        .map(|search| attribute(search, "outcome"))
        .collect();
    assert!(outcomes.contains(&Some("replaced".into())), "{outcomes:?}");
    assert_eq!(named("party.create").count(), 1, "{}", summary());

    // Each matched search: one trace from the queue to its ticket.
    let searches: Vec<&SpanData> = named("queue.search")
        .filter(|search| attribute(search, "outcome").as_deref() == Some("matched"))
        .collect();
    assert_eq!(searches.len(), 2, "{}", summary());
    for search in &searches {
        assert_eq!(attribute(search, "outcome").as_deref(), Some("matched"));
        let match_id = attribute(search, "match_id").expect("matched search names its match");
        let steps: Vec<String> = named("queue.step")
            .filter(|step| step.parent_span_id == search.span_context.span_id())
            .filter_map(|step| attribute(step, "step"))
            .collect();
        assert_eq!(
            steps,
            ["ready_check.offer", "ready_check.accept", "ticket.mint"],
            "{}",
            summary()
        );
        // The world server's span for the same match.
        let hello = named("ws.hello")
            .find(|hello| attribute(hello, "match_id").as_deref() == Some(match_id.as_str()))
            .unwrap_or_else(|| panic!("no hello for {match_id}: {}", summary()));
        assert_eq!(attribute(hello, "outcome").as_deref(), Some("admitted"));
    }

    // One match span; its seats join as its children.
    let matches: Vec<&SpanData> = named("match").collect();
    assert_eq!(matches.len(), 1, "{}", summary());
    let joins = named("seat.join")
        .filter(|join| join.parent_span_id == matches[0].span_context.span_id())
        .count();
    assert_eq!(joins, 2, "{}", summary());
    assert!(attribute(matches[0], "result").is_some(), "{}", summary());

    // No span or span event carries a ticket, a query, an unknown path or
    // an invite code.
    let everything = format!("{spans:?}");
    for ticket in &tickets {
        assert!(
            !everything.contains(ticket.as_str()),
            "a ticket leaked into a span"
        );
    }
    for secret in ["SECRETQUERY", "SECRETPATH", invite.as_str()] {
        assert!(!everything.contains(secret), "{secret} leaked into a span");
    }
}
