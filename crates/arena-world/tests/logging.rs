//! Process logs over a real match: lifecycle events carry the shared
//! structured fields, and no ticket, join link or signing key ever reaches a
//! log line, even with `--debug-json` wire logging on.

use std::io::Write;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use arena_protocol::{ClientMessage, PROTOCOL_VERSION, ServerMessage};
use arena_world::door::{self, CreatedMatch, DoorConfig};
use arena_world::ticket::{TicketKey, from_hex};
use arena_world::{Config, start};
use futures_util::{SinkExt, StreamExt};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

const KEY_HEX: &str = "5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed5eed";
const PAGE: &str = "http://localhost:8000/";
const WAIT: Duration = Duration::from_secs(5);

#[derive(Clone, Default)]
struct Captured(Arc<Mutex<Vec<u8>>>);

impl Write for Captured {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Captured {
    fn text(&self) -> String {
        String::from_utf8(self.0.lock().unwrap().clone()).unwrap()
    }
}

async fn create_match(addr: SocketAddr) -> CreatedMatch {
    let body = r#"{"team_size":1}"#;
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /api/matches HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    let (_, json) = response.split_once("\r\n\r\n").unwrap();
    serde_json::from_str(json).unwrap()
}

fn ticket_of(link: &str) -> String {
    let fragment = link.split_once('#').unwrap().1;
    fragment
        .split('&')
        .find_map(|pair| pair.strip_prefix("ticket="))
        .unwrap()
        .to_owned()
}

fn hello(ticket: &str) -> Message {
    Message::binary(
        ClientMessage::Hello {
            protocol: PROTOCOL_VERSION,
            ticket: from_hex(ticket).unwrap(),
            appearance: None,
        }
        .encode(),
    )
}

#[tokio::test]
async fn lifecycle_logs_are_structured_and_never_carry_tickets_or_keys() {
    let captured = Captured::default();
    let writer = captured.clone();
    let subscriber = tracing_subscriber::fmt()
        .json()
        .flatten_event(true)
        .with_env_filter("info")
        .with_writer(move || writer.clone())
        .finish();
    // This test binary holds only this test, so the global default is ours.
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let key = TicketKey::from_hex(KEY_HEX).unwrap();
    let mut server_config = Config::new(key.clone());
    server_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    server_config.tick = Duration::from_millis(5);
    server_config.debug_json = true;
    let server = start(server_config).await.unwrap();
    let mut door_config = DoorConfig::new(key, format!("ws://{}/play", server.addr), PAGE.into());
    door_config.bind = SocketAddr::from(([127, 0, 0, 1], 0));
    let door = door::start(door_config).await.unwrap();

    let created = create_match(door.addr).await;
    let tickets: Vec<String> = created
        .seats
        .iter()
        .map(|seat| ticket_of(&seat.link))
        .collect();
    let mut sockets = Vec::new();
    for ticket in &tickets {
        let (mut socket, _) = connect_async(format!("ws://{}/play", server.addr))
            .await
            .unwrap();
        socket.send(hello(ticket)).await.unwrap();
        let frame = tokio::time::timeout(WAIT, socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(
            ServerMessage::decode(&frame.into_data()).unwrap(),
            ServerMessage::Welcome(_)
        ));
        sockets.push(socket);
    }
    // A second Hello mid-match is wire-logged with its ticket redacted.
    sockets[0].send(hello(&tickets[0])).await.unwrap();
    // A browser error report that carries the page's ticket fragment and a
    // ticket in its message is logged scrubbed.
    let report = format!(
        r#"{{"kind":"wasm_panic","page":"{PAGE}#server=x&ticket={0}","message":"panicked near {0}"}}"#,
        tickets[0]
    );
    let mut stream = TcpStream::connect(door.addr).await.unwrap();
    let request = format!(
        "POST /api/telemetry HTTP/1.1\r\nHost: test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{report}",
        report.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).await.unwrap();
    assert!(response.starts_with("HTTP/1.1 204"), "{response}");
    // A queue player who says hello and closes normally is no refusal.
    let (mut leaver, _) = connect_async(format!("ws://{}/api/queue", door.addr))
        .await
        .unwrap();
    leaver
        .send(Message::text(
            r#"{"type":"hello","guest":"00000000000000000000000000000007"}"#,
        ))
        .await
        .unwrap();
    leaver.close(None).await.unwrap();
    // Garbage instead of a Hello: refused, with one throttled warning.
    for _ in 0..3 {
        let (mut garbage, _) = connect_async(format!("ws://{}/play", server.addr))
            .await
            .unwrap();
        garbage.send(Message::text("not a hello")).await.unwrap();
        let _ = tokio::time::timeout(WAIT, garbage.next()).await;
    }
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(sockets);
    tokio::time::sleep(Duration::from_millis(100)).await;
    door.stop().await.unwrap();
    server.stop().await.unwrap();

    let logs = captured.text();
    let lines: Vec<serde_json::Value> = logs
        .lines()
        .map(|line| serde_json::from_str(line).unwrap_or_else(|_| panic!("not JSON: {line}")))
        .collect();
    let find = |message: &str| {
        lines
            .iter()
            .find(|line| line["message"] == message)
            .unwrap_or_else(|| panic!("no {message:?} line in {logs}"))
    };
    let issued = find("match links issued");
    assert_eq!(issued["match_id"], created.match_id.as_str());
    assert_eq!(issued["team_size"], 1);
    let started = find("match started by its first connection");
    assert_eq!(started["match_id"], created.match_id.as_str());
    let joined = find("seat joined");
    assert_eq!(joined["match_id"], created.match_id.as_str());
    assert!(
        joined["seat"].is_u64() && joined["conn_id"].is_u64(),
        "{joined}"
    );
    // Wire lines keep the event text and put the message under `payload`.
    let hello = lines
        .iter()
        .find(|line| {
            line["message"] == "from client"
                && line["payload"]
                    .as_str()
                    .is_some_and(|p| p.contains("Hello"))
        })
        .unwrap_or_else(|| panic!("no wire Hello line in {logs}"));
    let payload: serde_json::Value =
        serde_json::from_str(hello["payload"].as_str().unwrap()).unwrap();
    assert_eq!(payload["Hello"]["ticket"], "<redacted>", "{hello}");
    assert_eq!(payload["Hello"]["protocol"], PROTOCOL_VERSION, "{hello}");
    assert!(
        lines.iter().any(|line| line["message"] == "to client"
            && line["payload"]
                .as_str()
                .is_some_and(|p| p.contains("Welcome"))),
        "{logs}"
    );

    let refusals: Vec<&serde_json::Value> = lines
        .iter()
        .filter(|line| line["reason"] == "refused" && line["level"] == "WARN")
        .collect();
    assert_eq!(refusals.len(), 1, "three refusals, one report: {logs}");
    assert_eq!(refusals[0]["error_code"], "Malformed");
    assert_eq!(refusals[0]["suppressed_reports"], 0);
    let panic = find("browser reported an error");
    assert_eq!(panic["kind"], "wasm_panic");
    assert_eq!(panic["page"], format!("{PAGE}<redacted>"));
    assert_eq!(panic["reason"], "panicked near <hex>");
    assert!(
        !logs.contains("closed a queue connection"),
        "a normal close was logged as a refusal: {logs}"
    );

    for ticket in &tickets {
        assert!(
            !logs.contains(ticket.as_str()),
            "a ticket leaked into {logs}"
        );
        // Nor as the byte list serde would print for the ticket's bytes.
        let bytes = from_hex(ticket).unwrap();
        let listed = format!("{bytes:?}").replace(' ', "");
        assert!(!logs.replace(' ', "").contains(&listed[1..listed.len() - 1]));
    }
    assert!(
        !logs.contains(KEY_HEX),
        "the signing key leaked into {logs}"
    );
    assert!(!logs.contains("#server="), "a join link leaked into {logs}");
}
