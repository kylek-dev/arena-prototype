//! `arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N]`
//!
//! The signing key comes from `ARENA_TICKET_KEY` (hex, at least 32 bytes),
//! shared with whatever issues tickets. With `--dev` and no key, a random
//! key is generated and `POST /dev/match` issues tickets itself.

use std::net::SocketAddr;
use std::process::ExitCode;
use std::time::Duration;

use arena_server::ticket::TicketKey;
use arena_server::{Config, start};

const USAGE: &str = "usage: arena-server [--bind ADDR] [--dev] [--debug-json] [--drain-seconds N]";

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("arena-server: {message}");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), String> {
    let mut bind: SocketAddr = SocketAddr::from(([127, 0, 0, 1], 8080));
    let mut dev = false;
    let mut debug_json = false;
    let mut drain = Duration::from_secs(60);
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--bind" => {
                let value = args.next().ok_or(USAGE)?;
                bind = value
                    .parse()
                    .map_err(|_| format!("invalid --bind {value}"))?;
            }
            "--dev" => dev = true,
            "--debug-json" => debug_json = true,
            "--drain-seconds" => {
                let value = args.next().ok_or(USAGE)?;
                drain = Duration::from_secs(
                    value
                        .parse()
                        .map_err(|_| format!("invalid --drain-seconds {value}"))?,
                );
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument {other}\n{USAGE}")),
        }
    }
    let key = match std::env::var("ARENA_TICKET_KEY") {
        Ok(hex) => TicketKey::from_hex(&hex)
            .ok_or("ARENA_TICKET_KEY must be hex encoding at least 32 bytes")?,
        Err(_) if dev => TicketKey::random(),
        Err(_) => return Err("ARENA_TICKET_KEY is required without --dev".into()),
    };
    let mut config = Config::new(key);
    config.bind = bind;
    config.dev = dev;
    config.debug_json = debug_json;
    let running = start(config).await.map_err(|error| error.to_string())?;
    eprintln!("arena-server: listening on ws://{}/play", running.addr);
    if dev {
        eprintln!(
            "arena-server: development mode; create a match with POST http://{}/dev/match?team_size=1",
            running.addr
        );
    }
    stop_signal().await;
    eprintln!("arena-server: draining for up to {}s", drain.as_secs());
    running
        .shut_down(drain)
        .await
        .map_err(|error| error.to_string())
}

async fn stop_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("SIGTERM handler");
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = terminate.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = tokio::signal::ctrl_c().await;
}
